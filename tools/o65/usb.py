#!/usr/bin/env python3
"""Tiny usbfs helper: control transfers + interrupt OUT without pyusb."""
import ctypes, fcntl, os, glob

def _IOC(d, t, nr, size): return (d << 30) | (size << 16) | (ord(t) << 8) | nr

class CtrlTransfer(ctypes.Structure):
    _fields_ = [("bRequestType", ctypes.c_uint8), ("bRequest", ctypes.c_uint8),
                ("wValue", ctypes.c_uint16), ("wIndex", ctypes.c_uint16),
                ("wLength", ctypes.c_uint16), ("timeout", ctypes.c_uint32),
                ("data", ctypes.c_void_p)]

class BulkTransfer(ctypes.Structure):
    _fields_ = [("ep", ctypes.c_uint), ("len", ctypes.c_uint),
                ("timeout", ctypes.c_uint), ("data", ctypes.c_void_p)]

USBDEVFS_CONTROL = _IOC(3, 'U', 0, ctypes.sizeof(CtrlTransfer))
USBDEVFS_BULK = _IOC(3, 'U', 2, ctypes.sizeof(BulkTransfer))
USBDEVFS_CLAIMINTERFACE = _IOC(2, 'U', 15, 4)
USBDEVFS_RELEASEINTERFACE = _IOC(2, 'U', 16, 4)

def find_device(vid, pid):
    for d in glob.glob("/sys/bus/usb/devices/*"):
        try:
            if int(open(d + "/idVendor").read(), 16) == vid and int(open(d + "/idProduct").read(), 16) == pid:
                bus = int(open(d + "/busnum").read()); dev = int(open(d + "/devnum").read())
                return f"/dev/bus/usb/{bus:03d}/{dev:03d}"
        except (FileNotFoundError, ValueError):
            pass
    raise RuntimeError(f"{vid:04x}:{pid:04x} not found")

class UsbDev:
    def __init__(self, path):
        self.fd = os.open(path, os.O_RDWR)
    def close(self): os.close(self.fd)
    def control(self, req_type, req, value, index, data_or_len, timeout=1000):
        if isinstance(data_or_len, int):
            buf = ctypes.create_string_buffer(data_or_len); n = data_or_len
        else:
            buf = ctypes.create_string_buffer(bytes(data_or_len), len(data_or_len)); n = len(data_or_len)
        ct = CtrlTransfer(req_type, req, value, index, n, timeout, ctypes.cast(buf, ctypes.c_void_p))
        r = fcntl.ioctl(self.fd, USBDEVFS_CONTROL, ct)
        return buf.raw[:r] if req_type & 0x80 else r
    def claim(self, intf): fcntl.ioctl(self.fd, USBDEVFS_CLAIMINTERFACE, ctypes.c_uint(intf))
    def release(self, intf): fcntl.ioctl(self.fd, USBDEVFS_RELEASEINTERFACE, ctypes.c_uint(intf))
    def interrupt_out(self, ep, data, timeout=1000):
        buf = ctypes.create_string_buffer(bytes(data), len(data))
        bt = BulkTransfer(ep, len(data), timeout, ctypes.cast(buf, ctypes.c_void_p))
        return fcntl.ioctl(self.fd, USBDEVFS_BULK, bt)
    # HID class requests
    def hid_report_descriptor(self, intf, length=256):
        return self.control(0x81, 0x06, 0x2200, intf, length)
    def hid_set_report(self, intf, report_type, report_id, data):
        return self.control(0x21, 0x09, (report_type << 8) | report_id, intf, data)
