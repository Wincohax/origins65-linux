use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};
use std::thread;
use std::time::{Duration, Instant};

use origins65::device::{DeviceError, Notification, Origins65, Transport, prepare_onboard_save};
use origins65::effects::{Engine, SYNC_FPS, parse_layer_spec, scale_frame};
use origins65::layout::key_by_usage;

use crate::model::DeviceInfo;

#[derive(Debug, Clone)]
pub enum DriverCommand {
    FetchInfo,
    SetBrightness {
        percent: u8,
    },
    SelectSlot {
        slot: u8,
    },
    StartLive {
        specs: Vec<String>,
        fps: u32,
        brightness: u8,
    },
    StopLive,
    Save {
        slot: u8,
        specs: Vec<String>,
        brightness: u8,
        activate: bool,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum DriverEvent {
    Busy(Option<String>),
    InfoLoaded(Result<DeviceInfo, String>),
    LiveState {
        running: bool,
        note: Option<String>,
    },
    LiveHardwareBrightness(u8),
    LiveExitedError(String),
    BrightnessApplied(Result<u8, String>),
    SlotSelected(Result<u8, String>),
    SaveProgress {
        #[allow(dead_code)]
        slot: u8,
        percent: u8,
        status: String,
    },
    SaveFinished(Result<String, String>),
}

/// Owns the single dedicated keyboard device thread (`Origins65`) so all USB interface 3 I/O
/// and the live rendering loop run in-process without contending for the device.
pub struct DriverClient {
    cmd_tx: Sender<DriverCommand>,
    shutdown_flag: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl DriverClient {
    pub fn spawn() -> (Self, Receiver<DriverEvent>) {
        Self::spawn_with_opener(Origins65::open)
    }

    pub fn spawn_with_opener<T, F>(opener: F) -> (Self, Receiver<DriverEvent>)
    where
        T: Transport + 'static,
        F: FnMut() -> Result<Origins65<T>, DeviceError> + Send + 'static,
    {
        let (cmd_tx, cmd_rx) = mpsc::channel::<DriverCommand>();
        let (evt_tx, evt_rx) = mpsc::channel::<DriverEvent>();
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        let shutdown_worker = Arc::clone(&shutdown_flag);
        let worker = thread::Builder::new()
            .name("origins65-driver".into())
            .spawn(move || {
                worker_loop(opener, cmd_rx, evt_tx, shutdown_worker);
            })
            .expect("failed to spawn driver worker thread");

        (
            Self {
                cmd_tx,
                shutdown_flag,
                worker: Some(worker),
            },
            evt_rx,
        )
    }

    pub fn send(&self, cmd: DriverCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn shutdown(&mut self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        let _ = self.cmd_tx.send(DriverCommand::Shutdown);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for DriverClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct LiveSession {
    engine: Engine,
    period: Duration,
    brightness: u8,
    next_tick: Instant,
}

fn ensure_device<'a, T, F>(
    dev: &'a mut Option<Origins65<T>>,
    opener: &mut F,
) -> Result<&'a mut Origins65<T>, String>
where
    T: Transport,
    F: FnMut() -> Result<Origins65<T>, DeviceError>,
{
    if dev.is_none() {
        let opened = opener().map_err(|e| e.to_string())?;
        *dev = Some(opened);
    }
    Ok(dev.as_mut().expect("device just opened"))
}

fn run_device_op<T, F, R, Op>(
    dev: &mut Option<Origins65<T>>,
    opener: &mut F,
    op: Op,
) -> Result<R, String>
where
    T: Transport,
    F: FnMut() -> Result<Origins65<T>, DeviceError>,
    Op: FnOnce(&mut Origins65<T>) -> Result<R, DeviceError>,
{
    let kb = ensure_device(dev, opener)?;
    match op(kb) {
        Ok(val) => Ok(val),
        Err(err) => {
            // Drop the handle so a disconnect or USB error recovers cleanly on next command.
            *dev = None;
            Err(err.to_string())
        }
    }
}

fn tick_live<T: Transport>(
    dev: &mut Option<Origins65<T>>,
    live: &mut Option<LiveSession>,
    evt_tx: &Sender<DriverEvent>,
) {
    let Some(session) = live.as_mut() else {
        return;
    };
    let Some(kb) = dev.as_mut() else {
        *live = None;
        let _ = evt_tx.send(DriverEvent::LiveState {
            running: false,
            note: None,
        });
        let _ = evt_tx.send(DriverEvent::LiveExitedError(
            "Keyboard handle closed unexpectedly.".into(),
        ));
        return;
    };

    let start = Instant::now();
    let step_res: Result<(), DeviceError> = (|| {
        for rep in kb.read_notifications(Duration::ZERO)? {
            match Notification::parse(&rep) {
                Some(Notification::KeyEvent {
                    usage,
                    pressed: true,
                }) => {
                    if let Some(key) = key_by_usage(usage) {
                        session.engine.key_pressed(key);
                    }
                }
                Some(Notification::BrightnessChanged { percent, .. }) => {
                    let _ = evt_tx.send(DriverEvent::LiveHardwareBrightness(percent));
                }
                _ => {}
            }
        }
        let frame = scale_frame(&session.engine.frame(), session.brightness as f64 / 100.0);
        kb.send_direct_frame(&frame)
    })();

    if let Err(err) = step_res {
        *live = None;
        *dev = None;
        let _ = evt_tx.send(DriverEvent::LiveState {
            running: false,
            note: None,
        });
        let _ = evt_tx.send(DriverEvent::LiveExitedError(err.to_string()));
    } else {
        session.next_tick = start + session.period;
    }
}

fn worker_loop<T, F>(
    mut opener: F,
    cmd_rx: Receiver<DriverCommand>,
    evt_tx: Sender<DriverEvent>,
    shutdown_flag: Arc<AtomicBool>,
) where
    T: Transport,
    F: FnMut() -> Result<Origins65<T>, DeviceError>,
{
    let mut dev: Option<Origins65<T>> = None;
    let mut live: Option<LiveSession> = None;

    loop {
        if shutdown_flag.load(Ordering::SeqCst) {
            break;
        }

        let cmd = if let Some(ref session) = live {
            let now = Instant::now();
            if now >= session.next_tick {
                tick_live(&mut dev, &mut live, &evt_tx);
                match cmd_rx.try_recv() {
                    Ok(c) => c,
                    Err(mpsc::TryRecvError::Empty) => continue,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            } else {
                let wait = session.next_tick.saturating_duration_since(now);
                match cmd_rx.recv_timeout(wait) {
                    Ok(c) => c,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        } else {
            match cmd_rx.recv() {
                Ok(c) => c,
                Err(_) => break,
            }
        };

        // Coalesce bursty StartLive / SetBrightness commands queued while busy.
        let mut cmd = cmd;
        while let Ok(next) = cmd_rx.try_recv() {
            match (&cmd, &next) {
                (DriverCommand::StartLive { .. }, DriverCommand::StartLive { .. })
                | (DriverCommand::SetBrightness { .. }, DriverCommand::SetBrightness { .. }) => {
                    cmd = next;
                }
                _ => {
                    if handle_command(&mut dev, &mut opener, cmd, &mut live, &evt_tx) {
                        return;
                    }
                    cmd = next;
                }
            }
        }

        if handle_command(&mut dev, &mut opener, cmd, &mut live, &evt_tx) {
            break;
        }
    }
}

/// Returns `true` if the worker should terminate.
fn handle_command<T, F>(
    dev: &mut Option<Origins65<T>>,
    opener: &mut F,
    cmd: DriverCommand,
    live: &mut Option<LiveSession>,
    evt_tx: &Sender<DriverEvent>,
) -> bool
where
    T: Transport,
    F: FnMut() -> Result<Origins65<T>, DeviceError>,
{
    match cmd {
        DriverCommand::Shutdown => return true,

        DriverCommand::FetchInfo => {
            let _ = evt_tx.send(DriverEvent::Busy(Some("Reading keyboard info...".into())));
            let res = run_device_op(dev, opener, |kb| kb.read_full_info());
            let ok = res.is_ok();
            let _ = evt_tx.send(DriverEvent::InfoLoaded(res));
            if let Some(session) = live.as_mut() {
                if ok {
                    session.next_tick = Instant::now();
                } else {
                    *live = None;
                    let _ = evt_tx.send(DriverEvent::LiveState {
                        running: false,
                        note: None,
                    });
                }
            }
            let _ = evt_tx.send(DriverEvent::Busy(None));
        }

        DriverCommand::SetBrightness { percent } => {
            let _ = evt_tx.send(DriverEvent::Busy(Some(format!(
                "Setting hardware brightness to {percent}%..."
            ))));
            let res = run_device_op(dev, opener, |kb| kb.set_brightness(percent as i32))
                .map(|_| percent);
            let ok = res.is_ok();
            let _ = evt_tx.send(DriverEvent::BrightnessApplied(res));
            if ok {
                let info = run_device_op(dev, opener, |kb| kb.read_full_info());
                let _ = evt_tx.send(DriverEvent::InfoLoaded(info));
            }
            if let Some(session) = live.as_mut() {
                if dev.is_some() {
                    session.next_tick = Instant::now();
                } else {
                    *live = None;
                    let _ = evt_tx.send(DriverEvent::LiveState {
                        running: false,
                        note: None,
                    });
                }
            }
            let _ = evt_tx.send(DriverEvent::Busy(None));
        }

        DriverCommand::SelectSlot { slot } => {
            let _ = evt_tx.send(DriverEvent::Busy(Some(format!(
                "Switching to onboard slot {slot}..."
            ))));
            if live.take().is_some() {
                let _ = evt_tx.send(DriverEvent::LiveState {
                    running: false,
                    note: Some(format!("Live preview stopped to show onboard slot {slot}.")),
                });
            }

            let res = run_device_op(dev, opener, |kb| kb.select_slot(slot, false)).map(|()| slot);
            let ok = res.is_ok();
            let _ = evt_tx.send(DriverEvent::SlotSelected(res));
            if ok {
                let info = run_device_op(dev, opener, |kb| kb.read_full_info());
                let _ = evt_tx.send(DriverEvent::InfoLoaded(info));
            }
            let _ = evt_tx.send(DriverEvent::Busy(None));
        }

        DriverCommand::StartLive {
            specs,
            fps,
            brightness,
        } => {
            if specs.is_empty() {
                *live = None;
                let _ = evt_tx.send(DriverEvent::LiveState {
                    running: false,
                    note: None,
                });
                let _ = evt_tx.send(DriverEvent::LiveExitedError(
                    "Cannot start live preview with no layers.".into(),
                ));
                return false;
            }
            let mut layers = Vec::with_capacity(specs.len());
            for spec in &specs {
                match parse_layer_spec(spec) {
                    Ok((layer, _)) => layers.push(layer),
                    Err(err) => {
                        *live = None;
                        let _ = evt_tx.send(DriverEvent::LiveState {
                            running: false,
                            note: None,
                        });
                        let _ = evt_tx.send(DriverEvent::LiveExitedError(err));
                        return false;
                    }
                }
            }
            if let Err(err) = ensure_device(dev, opener) {
                *live = None;
                let _ = evt_tx.send(DriverEvent::LiveState {
                    running: false,
                    note: None,
                });
                let _ = evt_tx.send(DriverEvent::LiveExitedError(err));
                return false;
            }
            let period = Duration::from_secs_f64(1.0 / (fps.max(1) as f64));
            *live = Some(LiveSession {
                engine: Engine::new(layers),
                period,
                brightness,
                next_tick: Instant::now(),
            });
            let _ = evt_tx.send(DriverEvent::LiveState {
                running: true,
                note: None,
            });
        }

        DriverCommand::StopLive => {
            *live = None;
            let _ = evt_tx.send(DriverEvent::LiveState {
                running: false,
                note: None,
            });
        }

        DriverCommand::Save {
            slot,
            specs,
            brightness,
            activate,
        } => {
            let _ = evt_tx.send(DriverEvent::Busy(Some(format!(
                "Uploading animation to onboard slot {slot}..."
            ))));
            if live.take().is_some() {
                let _ = evt_tx.send(DriverEvent::LiveState {
                    running: false,
                    note: Some(format!("Live preview stopped for upload to slot {slot}.")),
                });
            }

            let save_res: Result<String, String> = (|| {
                if specs.is_empty() {
                    return Err("Add at least one layer before saving.".into());
                }
                let mut layers = Vec::with_capacity(specs.len());
                for spec in &specs {
                    let (layer, _) = parse_layer_spec(spec)?;
                    layers.push(layer);
                }
                let prepared = prepare_onboard_save(&layers, brightness as f64)?;
                let secs = (prepared.frames.len() as f64) / (SYNC_FPS as f64);
                let trig_suffix = match &prepared.trigger {
                    Some(t) => format!(", onboard key-press effect: {}", t.kind.name()),
                    None => String::new(),
                };
                let _ = evt_tx.send(DriverEvent::SaveProgress {
                    slot,
                    percent: 0,
                    status: format!(
                        "rendered {} frames ({secs:.1} s loop at {SYNC_FPS} fps){trig_suffix}",
                        prepared.frames.len()
                    ),
                });

                let mut last_pct: Option<u8> = None;
                run_device_op(dev, opener, |kb| {
                    kb.save_prepared(slot, &prepared, activate, |done, total| {
                        let pct = ((done * 100) / total).min(100) as u8;
                        if last_pct != Some(pct) || done == total {
                            last_pct = Some(pct);
                            let _ = evt_tx.send(DriverEvent::SaveProgress {
                                slot,
                                percent: pct,
                                status: format!("uploading to slot {slot}: {pct:3}%"),
                            });
                        }
                    })
                })?;

                Ok(format!(
                    "saved to onboard slot {slot}{}",
                    if activate { " and activated" } else { "" }
                ))
            })();

            let ok = save_res.is_ok();
            let _ = evt_tx.send(DriverEvent::SaveFinished(save_res));
            if ok {
                let info = run_device_op(dev, opener, |kb| kb.read_full_info());
                let _ = evt_tx.send(DriverEvent::InfoLoaded(info));
            }
            let _ = evt_tx.send(DriverEvent::Busy(None));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct SharedMockState {
        direct_frames: usize,
        brightness_writes: Vec<u8>,
        slot_writes: Vec<u8>,
        upload_cmds: Vec<u8>,
        notifications: VecDeque<Vec<u8>>,
        fail_next_set: Option<String>,
        current_profile: [u8; 64],
    }

    struct MockTransport {
        state: Arc<Mutex<SharedMockState>>,
        last_cmd: u8,
        get_queue: VecDeque<[u8; 64]>,
        ack_queue: VecDeque<Vec<u8>>,
    }

    impl MockTransport {
        fn new(state: Arc<Mutex<SharedMockState>>) -> Self {
            Self {
                state,
                last_cmd: 0,
                get_queue: VecDeque::new(),
                ack_queue: VecDeque::new(),
            }
        }
    }

    impl Transport for MockTransport {
        fn set_feature_report(&mut self, report: &[u8; 64]) -> Result<(), DeviceError> {
            let mut st = self.state.lock().unwrap();
            if let Some(msg) = st.fail_next_set.take() {
                return Err(DeviceError::Io(msg));
            }
            if report[0] == 0x04 {
                self.last_cmd = report[1];
                if report[1] == 0xF2 {
                    st.direct_frames += 1;
                } else {
                    self.ack_queue.push_back(vec![0x05, 0xAC, 0xAC, 0xAC]);
                    if matches!(report[1], 0x53 | 0x63 | 0x73) {
                        st.upload_cmds.push(report[1]);
                    }
                }
            } else if self.last_cmd != 0xF2 {
                self.ack_queue.push_back(vec![0x05, 0xAC, 0xAC, 0xAC]);
                if self.last_cmd == 0x57 && report[62] == 0xAA && report[63] == 0x55 {
                    st.current_profile = *report;
                    st.brightness_writes.push(report[12]);
                    st.slot_writes.push(report[0]);
                }
            }
            Ok(())
        }

        fn get_feature_report(&mut self) -> Result<[u8; 64], DeviceError> {
            if let Some(pkt) = self.get_queue.pop_front() {
                return Ok(pkt);
            }
            let st = self.state.lock().unwrap();
            let mut resp = [0u8; 64];
            resp[1] = self.last_cmd;
            resp[3] = 1;
            match self.last_cmd {
                0x05 => {
                    let mut info = [0u8; 64];
                    info[8] = 0xF0;
                    info[9] = 0x03;
                    info[12] = 0x8F;
                    info[13] = 0x03;
                    info[17] = 0x04;
                    info[18] = 0x21;
                    info[24] = 57;
                    self.get_queue.push_back(info);
                }
                0x56 => {
                    self.get_queue.push_back(st.current_profile);
                }
                0x26 | 0x36 | 0x46 => {
                    let slot = (self.last_cmd - 0x26) / 16;
                    let mut prof = [0u8; 64];
                    prof[0] = slot;
                    prof[12] = 255;
                    self.get_queue.push_back(prof);
                }
                _ => {}
            }
            Ok(resp)
        }

        fn has_notifications(&self) -> bool {
            true
        }

        fn read_notification(
            &mut self,
            _timeout: Duration,
        ) -> Result<Option<Vec<u8>>, DeviceError> {
            let mut st = self.state.lock().unwrap();
            if let Some(rep) = st.notifications.pop_front() {
                return Ok(Some(rep));
            }
            drop(st);
            Ok(self.ack_queue.pop_front())
        }
    }

    #[test]
    fn in_process_driver_runs_live_info_brightness_save_and_errors() {
        let mut init_prof = [0u8; 64];
        init_prof[0] = 1;
        init_prof[12] = 204; // 80%
        let state = Arc::new(Mutex::new(SharedMockState {
            direct_frames: 0,
            brightness_writes: Vec::new(),
            slot_writes: Vec::new(),
            upload_cmds: Vec::new(),
            notifications: VecDeque::new(),
            fail_next_set: None,
            current_profile: init_prof,
        }));

        let opener_state = Arc::clone(&state);
        let (mut client, rx) = DriverClient::spawn_with_opener(move || {
            let mut kb = Origins65::with_transport(MockTransport::new(Arc::clone(&opener_state)));
            kb.set_timeouts(
                Duration::from_millis(50),
                Duration::from_millis(200),
                Duration::ZERO,
            );
            Ok(kb)
        });

        // 1. StartLive streams frames and forwards Fn brightness notifications.
        client.send(DriverCommand::StartLive {
            specs: vec!["wave".into()],
            fps: 60,
            brightness: 90,
        });
        let mut saw_live = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(DriverEvent::LiveState { running: true, .. }) =
                rx.recv_timeout(Duration::from_millis(100))
            {
                saw_live = true;
                break;
            }
        }
        assert!(saw_live, "expected live to start");

        // Inject a hardware brightness change notification (Fn+] -> 75%)
        state.lock().unwrap().notifications.push_back(vec![0x05, 0x08, 191]);
        let mut saw_hw_pct = None;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(DriverEvent::LiveHardwareBrightness(pct)) =
                rx.recv_timeout(Duration::from_millis(100))
            {
                saw_hw_pct = Some(pct);
                break;
            }
        }
        assert_eq!(saw_hw_pct, Some(75));
        assert!(state.lock().unwrap().direct_frames > 0);

        // 2. FetchInfo while live keeps live running and loads device info.
        client.send(DriverCommand::FetchInfo);
        let mut got_info = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(DriverEvent::InfoLoaded(Ok(info))) =
                rx.recv_timeout(Duration::from_millis(100))
            {
                assert_eq!(info.firmware, "21.04");
                assert_eq!(info.current.brightness_percent, 80);
                assert_eq!(info.slots.len(), 3);
                got_info = true;
                break;
            }
        }
        assert!(got_info);

        // 3. Save while live stops live, reports upload progress to 100%, and activates slot 0.
        client.send(DriverCommand::Save {
            slot: 0,
            specs: vec!["solid:color=ff0000".into()],
            brightness: 100,
            activate: true,
        });
        let mut percents = Vec::new();
        let mut save_ok = false;
        let mut stopped_note = None;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !save_ok {
            if let Ok(ev) = rx.recv_timeout(Duration::from_millis(100)) {
                match ev {
                    DriverEvent::LiveState {
                        running: false,
                        note,
                    } => stopped_note = note,
                    DriverEvent::SaveProgress { percent, .. } if percent > 0 => {
                        percents.push(percent);
                    }
                    DriverEvent::SaveFinished(Ok(msg)) => {
                        assert!(msg.contains("saved to onboard slot 0 and activated"));
                        save_ok = true;
                    }
                    _ => {}
                }
            }
        }
        assert!(save_ok);
        assert!(stopped_note.is_some());
        assert!(percents.contains(&100));
        assert_eq!(state.lock().unwrap().upload_cmds, vec![0x53]);

        // The worker refreshes device info right after SaveFinished; wait for it so the injected
        // failure below is consumed by SelectSlot and not by that refresh.
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(DriverEvent::InfoLoaded(_)) = rx.recv_timeout(Duration::from_millis(100)) {
                break;
            }
        }

        // 4. Error propagation on transport failure
        state.lock().unwrap().fail_next_set = Some("simulated usb disconnect".into());
        client.send(DriverCommand::SelectSlot { slot: 2 });
        let mut saw_err = None;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(DriverEvent::SlotSelected(Err(e))) =
                rx.recv_timeout(Duration::from_millis(100))
            {
                saw_err = Some(e);
                break;
            }
        }
        assert_eq!(saw_err.as_deref(), Some("simulated usb disconnect"));

        client.shutdown();
    }
}
