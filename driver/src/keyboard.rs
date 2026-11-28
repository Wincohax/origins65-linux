//! One interface over every supported keyboard, for the CLI and the GUI.

use std::time::Duration;

use crate::core::{OriginsCore, find_core_hidraw};
use crate::device::{
    DeviceError, DeviceInfo, FullDeviceInfo, Notification, Origins65, PreparedSave, Profile,
    Transport,
};
use crate::effects::Frame;
use crate::model::Model;

/// Operations both protocols support.
pub trait Keyboard: Send {
    fn model(&self) -> Model;

    fn device_info(&mut self) -> Result<DeviceInfo, DeviceError>;

    /// Current profile (`None`) or the profile of an onboard slot.
    fn read_profile(&mut self, slot: Option<u8>) -> Result<Profile, DeviceError>;

    /// Device info plus current and per-slot profiles (`info --json`).
    fn read_full_info(&mut self) -> Result<FullDeviceInfo, DeviceError>;

    /// Hardware brightness in percent; returns the resulting current profile.
    fn set_brightness(&mut self, percent: i32) -> Result<Profile, DeviceError>;

    fn active_slot(&mut self) -> Result<u8, DeviceError>;

    fn select_slot(&mut self, slot: u8, force_reload: bool) -> Result<(), DeviceError>;

    /// Key presses, brightness and game-mode changes reported by the keyboard.
    fn poll_events(&mut self, timeout: Duration) -> Result<Vec<Notification>, DeviceError>;

    /// One volatile live frame; the keyboard falls back to its onboard lighting when frames stop.
    fn send_direct_frame(&mut self, frame: &Frame) -> Result<(), DeviceError>;

    /// Store a prepared animation (see [`crate::device::prepare_onboard_save`]) in a slot and make
    /// sure the right slot is showing afterwards. Returns a non-fatal note.
    fn save_prepared(
        &mut self,
        slot: u8,
        prepared: &PreparedSave,
        activate: bool,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Option<String>, DeviceError>;
}

impl<T: Transport> Keyboard for Origins65<T> {
    fn model(&self) -> Model {
        Model::Origins65
    }

    fn device_info(&mut self) -> Result<DeviceInfo, DeviceError> {
        Origins65::device_info(self)
    }

    fn read_profile(&mut self, slot: Option<u8>) -> Result<Profile, DeviceError> {
        Origins65::read_profile(self, slot, false)
    }

    fn read_full_info(&mut self) -> Result<FullDeviceInfo, DeviceError> {
        Origins65::read_full_info(self)
    }

    fn set_brightness(&mut self, percent: i32) -> Result<Profile, DeviceError> {
        Origins65::set_brightness(self, percent)
    }

    fn active_slot(&mut self) -> Result<u8, DeviceError> {
        Origins65::active_slot(self)
    }

    fn select_slot(&mut self, slot: u8, force_reload: bool) -> Result<(), DeviceError> {
        Origins65::select_slot(self, slot, force_reload)
    }

    fn poll_events(&mut self, timeout: Duration) -> Result<Vec<Notification>, DeviceError> {
        Ok(self
            .read_notifications(timeout)?
            .iter()
            .filter_map(|rep| Notification::parse(rep))
            .collect())
    }

    fn send_direct_frame(&mut self, frame: &Frame) -> Result<(), DeviceError> {
        Origins65::send_direct_frame(self, frame)
    }

    fn save_prepared(
        &mut self,
        slot: u8,
        prepared: &PreparedSave,
        activate: bool,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Option<String>, DeviceError> {
        Origins65::save_prepared(self, slot, prepared, activate, progress)
    }
}

impl<T: crate::core::HidTransport> Keyboard for OriginsCore<T> {
    fn model(&self) -> Model {
        Model::OriginsCore
    }

    fn device_info(&mut self) -> Result<DeviceInfo, DeviceError> {
        OriginsCore::device_info(self)
    }

    fn read_profile(&mut self, slot: Option<u8>) -> Result<Profile, DeviceError> {
        OriginsCore::read_profile(self, slot)
    }

    fn read_full_info(&mut self) -> Result<FullDeviceInfo, DeviceError> {
        OriginsCore::read_full_info(self)
    }

    fn set_brightness(&mut self, percent: i32) -> Result<Profile, DeviceError> {
        OriginsCore::set_brightness(self, percent)
    }

    fn active_slot(&mut self) -> Result<u8, DeviceError> {
        OriginsCore::active_slot(self)
    }

    fn select_slot(&mut self, slot: u8, force_reload: bool) -> Result<(), DeviceError> {
        OriginsCore::select_slot(self, slot, force_reload)
    }

    fn poll_events(&mut self, timeout: Duration) -> Result<Vec<Notification>, DeviceError> {
        OriginsCore::poll_events(self, timeout)
    }

    fn send_direct_frame(&mut self, frame: &Frame) -> Result<(), DeviceError> {
        OriginsCore::send_direct_frame(self, frame)
    }

    fn save_prepared(
        &mut self,
        slot: u8,
        prepared: &PreparedSave,
        activate: bool,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Option<String>, DeviceError> {
        OriginsCore::save_prepared(self, slot, prepared, activate, progress)
    }
}

/// Whether a keyboard of this model is plugged in (does not open it).
pub fn is_connected(model: Model) -> bool {
    match model {
        Model::Origins65 => nusb::list_devices().is_ok_and(|mut devs| {
            devs.any(|d| model.usb_ids().contains(&(d.vendor_id(), d.product_id())))
        }),
        Model::OriginsCore => find_core_hidraw().is_some(),
    }
}

/// The model of the connected keyboard (the Origins 65 wins if both are plugged in).
pub fn detect_model() -> Option<Model> {
    Model::ALL.into_iter().find(|&m| is_connected(m))
}

/// Open the keyboard of `model`, or whichever supported keyboard is connected.
pub fn open_keyboard(model: Option<Model>) -> Result<Box<dyn Keyboard>, DeviceError> {
    let model = match model {
        Some(m) => m,
        None => detect_model().ok_or(DeviceError::NoKeyboard)?,
    };
    Ok(match model {
        Model::Origins65 => Box::new(Origins65::open()?),
        Model::OriginsCore => Box::new(OriginsCore::open()?),
    })
}
