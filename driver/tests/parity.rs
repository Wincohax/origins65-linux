use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use origins65::device::{
    DeviceError, Notification, OnboardTrigger, Origins65, Profile, Transport,
    brightness_byte_to_percent, percent_to_brightness_byte, prepare_onboard_save,
};
use origins65::effects::{
    EffectKind, Engine, Frame, MAX_SYNC_FRAMES, SYNC_FPS, SYNC_INTERVAL, parse_layer_spec,
    render_sync_frames, scale_frame,
};
use origins65::layout::{KEYS, NUM_LEDS, key_by_name};

#[derive(Deserialize)]
struct RefKey {
    name: String,
    usage: Option<u8>,
    row: u8,
    col: u8,
    led: usize,
}

#[derive(Deserialize)]
struct RefCase {
    name: String,
    specs: Vec<String>,
    brightness: f64,
    frames: Vec<Vec<[u8; 3]>>,
}

#[derive(Deserialize)]
struct RefProtocol {
    direct_frame: Vec<Vec<u8>>,
    write_profile_current: Vec<Vec<u8>>,
    write_profile_slot0: Vec<Vec<u8>>,
    write_profile_slot1: Vec<Vec<u8>>,
    write_profile_slot2: Vec<Vec<u8>>,
    write_profile_app: Vec<Vec<u8>>,
    brightness_bytes: HashMap<String, u8>,
    brightness_percents: HashMap<String, u8>,
    select_slot_force_reload_active: Vec<Vec<u8>>,
    select_slot_force_reload_different: Vec<Vec<u8>>,
    upload_slot0_no_trigger: Vec<Vec<u8>>,
    upload_slot1_fade: Vec<Vec<u8>>,
    upload_slot2_explosion_random: Vec<Vec<u8>>,
    upload_slot0_flame: Vec<Vec<u8>>,
}

#[derive(Deserialize)]
struct ReferenceDump {
    keys: Vec<RefKey>,
    effect_lines: Vec<String>,
    deterministic_cases: Vec<RefCase>,
    protocol: RefProtocol,
}

fn load_reference() -> ReferenceDump {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/dump_reference.py");
    let output = Command::new("python3")
        .arg(&script)
        .output()
        .expect("failed to run dump_reference.py");
    assert!(
        output.status.success(),
        "dump_reference.py failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("failed to parse dump_reference.py JSON")
}

struct MockTransport {
    sent: Vec<[u8; 64]>,
    last_cmd: u8,
    profiles: HashMap<u8, [u8; 64]>,
    get_queue: VecDeque<[u8; 64]>,
    notifications: VecDeque<Vec<u8>>,
    auto_ack: bool,
    has_notif: bool,
}

impl MockTransport {
    fn new() -> Self {
        Self {
            sent: Vec::new(),
            last_cmd: 0,
            profiles: HashMap::new(),
            get_queue: VecDeque::new(),
            notifications: VecDeque::new(),
            auto_ack: true,
            has_notif: true,
        }
    }

    fn with_profile(mut self, read_cmd: u8, raw: [u8; 64]) -> Self {
        self.profiles.insert(read_cmd, raw);
        self
    }

    fn sent_as_vecs(&self) -> Vec<Vec<u8>> {
        self.sent.iter().map(|b| b.to_vec()).collect()
    }
}

impl Transport for MockTransport {
    fn set_feature_report(&mut self, report: &[u8; 64]) -> Result<(), DeviceError> {
        self.sent.push(*report);
        if report[0] == 0x04 {
            self.last_cmd = report[1];
        }
        Ok(())
    }

    fn get_feature_report(&mut self) -> Result<[u8; 64], DeviceError> {
        if let Some(pkt) = self.get_queue.pop_front() {
            return Ok(pkt);
        }
        let mut resp = [0u8; 64];
        resp[1] = self.last_cmd;
        resp[3] = 1;
        if let Some(&prof) = self.profiles.get(&self.last_cmd) {
            self.get_queue.push_back(prof);
        }
        Ok(resp)
    }

    fn has_notifications(&self) -> bool {
        self.has_notif
    }

    fn read_notification(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>, DeviceError> {
        if let Some(rep) = self.notifications.pop_front() {
            return Ok(Some(rep));
        }
        if self.auto_ack {
            return Ok(Some(vec![0x05, 0xAC, 0xAC, 0xAC]));
        }
        Ok(None)
    }
}

#[test]
fn layout_matches_python_exactly() {
    let reference = load_reference();
    assert_eq!(KEYS.len(), reference.keys.len());
    for (rust_k, py_k) in KEYS.iter().zip(reference.keys.iter()) {
        assert_eq!(rust_k.name, py_k.name);
        assert_eq!(rust_k.usage, py_k.usage);
        assert_eq!(rust_k.row, py_k.row);
        assert_eq!(rust_k.col, py_k.col);
        assert_eq!(rust_k.led, py_k.led);
    }
}

#[test]
fn effects_cli_listing_matches_python_exactly() {
    let reference = load_reference();
    let rust_lines: Vec<String> = EffectKind::ALL
        .into_iter()
        .map(|kind| {
            let kind_str = if kind.is_triggered() {
                if kind.onboard_trigger_code().is_some() {
                    "key-press (onboard ok)"
                } else {
                    "key-press (live only)"
                }
            } else {
                "looped"
            };
            format!(
                "{:<10} {:<22} speed {}-{} ms, default {}",
                kind.name(),
                kind_str,
                kind.min_speed(),
                kind.max_speed(),
                kind.default_speed()
            )
        })
        .collect();
    assert_eq!(rust_lines, reference.effect_lines);
}

#[test]
fn deterministic_render_sync_frames_exact_parity() {
    let reference = load_reference();
    assert!(
        reference.deterministic_cases.len() >= 30,
        "expected >= 30 deterministic parity configs"
    );

    for case in &reference.deterministic_cases {
        let layers: Vec<_> = case
            .specs
            .iter()
            .map(|s| parse_layer_spec(s).expect("parse layer").0)
            .collect();
        let mut frames = render_sync_frames(&layers, SYNC_FPS, MAX_SYNC_FRAMES);
        if (case.brightness - 100.0).abs() > f64::EPSILON {
            let k = case.brightness / 100.0;
            frames = frames.iter().map(|f| scale_frame(f, k)).collect();
        }

        assert_eq!(
            frames.len(),
            case.frames.len(),
            "frame count mismatch for case '{}'",
            case.name
        );

        for (fi, (rust_frame, py_frame)) in frames.iter().zip(case.frames.iter()).enumerate() {
            assert_eq!(py_frame.len(), NUM_LEDS);
            for (led, (&(rr, rg, rb), &[pr, pg, pb])) in
                rust_frame.iter().zip(py_frame.iter()).enumerate()
            {
                assert_eq!(
                    (rr, rg, rb),
                    (pr, pg, pb),
                    "pixel mismatch in case '{}' frame {} led {}",
                    case.name,
                    fi,
                    led
                );
            }
        }
    }
}

#[test]
fn random_effects_structural_checks() {
    // 1. Confetti: frame count, key mask, and loop closure
    let (confetti_all, _) = parse_layer_spec("confetti:speed=1000").unwrap();
    let frames = render_sync_frames(&[confetti_all], SYNC_FPS, MAX_SYNC_FRAMES);
    // fpc = 20, cycle_count = 4 -> 80 frames
    assert_eq!(frames.len(), 80);
    // Loop closure: last frame ends on `first[led]` which equals frame 0!
    assert_eq!(
        frames[0],
        frames[frames.len() - 1],
        "confetti onboard loop must close cleanly (first frame == last frame)"
    );
    let key_leds: HashSet<usize> = KEYS.iter().map(|k| k.led).collect();
    for frame in &frames {
        for (led, &rgb) in frame.iter().enumerate() {
            if !key_leds.contains(&led) {
                assert_eq!(rgb, (0, 0, 0), "unused LED slot {led} must stay off");
            }
        }
    }

    // Masked confetti
    let (confetti_wasd, _) = parse_layer_spec("confetti:speed=1000,keys=w+a+s+d").unwrap();
    let wasd_leds: HashSet<usize> = ["w", "a", "s", "d"]
        .into_iter()
        .map(|n| key_by_name(n).unwrap().led)
        .collect();
    let frames_wasd = render_sync_frames(&[confetti_wasd], SYNC_FPS, MAX_SYNC_FRAMES);
    assert_eq!(frames_wasd.len(), 80);
    assert_eq!(frames_wasd[0], frames_wasd[frames_wasd.len() - 1]);
    for frame in &frames_wasd {
        for (led, &rgb) in frame.iter().enumerate() {
            if !wasd_leds.contains(&led) {
                assert_eq!(rgb, (0, 0, 0));
            }
        }
    }

    // 2. Sun: frame count, palette bounds (r==255, b==0 on masked keys), mask respect
    let (sun_layer, _) = parse_layer_spec("sun:speed=1000,keys=1+2+3").unwrap();
    let num_leds: HashSet<usize> = ["1", "2", "3"]
        .into_iter()
        .map(|n| key_by_name(n).unwrap().led)
        .collect();
    let sun_frames = render_sync_frames(&[sun_layer], SYNC_FPS, MAX_SYNC_FRAMES);
    assert_eq!(sun_frames.len(), 80);
    for frame in &sun_frames {
        for (led, &(r, g, b)) in frame.iter().enumerate() {
            if num_leds.contains(&led) {
                assert_eq!(r, 255, "sun theme always has R=255");
                assert!(g <= 183, "sun theme G out of range: {g}");
                assert_eq!(b, 0, "sun theme always has B=0");
            } else {
                assert_eq!((r, g, b), (0, 0, 0));
            }
        }
    }

    // 3. Twilight: frame count (fpc=10 * 8 = 80 for speed=500), mask respect, non-empty illumination
    let (twilight_layer, _) = parse_layer_spec("twilight:speed=500,color=0080ff,keys=w+a+s+d").unwrap();
    let tw_frames = render_sync_frames(&[twilight_layer], SYNC_FPS, MAX_SYNC_FRAMES);
    assert_eq!(tw_frames.len(), 80);
    let mut any_lit = false;
    for frame in &tw_frames {
        for (led, &(r, g, b)) in frame.iter().enumerate() {
            if !wasd_leds.contains(&led) {
                assert_eq!((r, g, b), (0, 0, 0));
            } else if r > 0 || g > 0 || b > 0 {
                any_lit = true;
                assert_eq!(r, 0, "fixed color 0080ff must keep R=0");
                assert!(b >= g, "fixed color 0080ff must keep B >= G");
            }
        }
    }
    assert!(any_lit, "twilight must illuminate at least one key");

    // 4. Random color breathing & swipe
    let (breath_rand, _) = parse_layer_spec("breathing:color=random,speed=1000,keys=esc").unwrap();
    let br_frames = render_sync_frames(&[breath_rand], SYNC_FPS, MAX_SYNC_FRAMES);
    assert_eq!(br_frames.len(), 20);
    let esc_led = key_by_name("esc").unwrap().led;
    for frame in &br_frames {
        for (led, &rgb) in frame.iter().enumerate() {
            if led != esc_led {
                assert_eq!(rgb, (0, 0, 0));
            }
        }
    }

    let (swipe_rand, _) = parse_layer_spec("swipe:color=random,speed=1000").unwrap();
    let sw_frames = render_sync_frames(&[swipe_rand], SYNC_FPS, MAX_SYNC_FRAMES);
    assert_eq!(sw_frames.len(), 40);
}

#[test]
fn protocol_packet_bytes_match_python_exactly() {
    let reference = load_reference();
    let proto = &reference.protocol;

    // 1. Direct frame
    let mut frame: Frame = [(0, 0, 0); NUM_LEDS];
    for (i, slot) in frame.iter_mut().enumerate() {
        *slot = (((i * 7) & 0xFF) as u8, ((i * 13) & 0xFF) as u8, ((i * 29) & 0xFF) as u8);
    }
    let mut kb = Origins65::with_transport(MockTransport::new());
    kb.send_direct_frame(&frame).unwrap();
    assert_eq!(kb.transport().sent_as_vecs(), proto.direct_frame);

    // 2. Profile writes preserving unknown bytes
    let mut base_raw = [0u8; 64];
    for (i, b) in base_raw.iter_mut().enumerate() {
        *b = i as u8;
    }
    base_raw[0] = 1;
    base_raw[4] = 1;
    base_raw[8] = 5;
    base_raw[12] = 200;
    base_raw[62] = 0x11;
    base_raw[63] = 0x22;
    let prof = Profile::from_raw(base_raw);

    let profile_cases = [
        (None, false, &proto.write_profile_current),
        (Some(0), false, &proto.write_profile_slot0),
        (Some(1), false, &proto.write_profile_slot1),
        (Some(2), false, &proto.write_profile_slot2),
        (None, true, &proto.write_profile_app),
    ];
    for (slot, app_mode, expected) in profile_cases {
        let mut kb = Origins65::with_transport(MockTransport::new());
        kb.write_profile(&prof, slot, app_mode).unwrap();
        assert_eq!(kb.transport().sent_as_vecs(), *expected);
    }

    // 3. Brightness conversions (banker's rounding parity)
    for (k, &expected) in &proto.brightness_bytes {
        let pct: i32 = k.parse().unwrap();
        assert_eq!(
            percent_to_brightness_byte(pct),
            expected,
            "percent_to_brightness_byte({pct})"
        );
    }
    for (k, &expected) in &proto.brightness_percents {
        let b: u8 = k.parse().unwrap();
        assert_eq!(
            brightness_byte_to_percent(b),
            expected,
            "brightness_byte_to_percent({b})"
        );
    }

    // 4. select_slot force_reload
    let mut cur_raw = [0u8; 64];
    cur_raw[0] = 1;
    cur_raw[12] = 180;
    cur_raw[33] = 0xAB;

    let mut kb = Origins65::with_transport(MockTransport::new().with_profile(0x56, cur_raw));
    kb.select_slot(1, true).unwrap();
    assert_eq!(
        kb.transport().sent_as_vecs(),
        proto.select_slot_force_reload_active
    );

    let mut kb = Origins65::with_transport(MockTransport::new().with_profile(0x56, cur_raw));
    kb.select_slot(2, true).unwrap();
    assert_eq!(
        kb.transport().sent_as_vecs(),
        proto.select_slot_force_reload_different
    );

    // 5. upload_animation
    let mut f0: Frame = [(0, 0, 0); NUM_LEDS];
    let mut f1: Frame = [(0, 0, 0); NUM_LEDS];
    for i in 0..NUM_LEDS {
        f0[i] = (((i * 3) & 0xFF) as u8, ((i * 5) & 0xFF) as u8, ((i * 11) & 0xFF) as u8);
        f1[i] = (((255 - i) & 0xFF) as u8, ((i * 2) & 0xFF) as u8, ((i * 4) & 0xFF) as u8);
    }

    let mut kb = Origins65::with_transport(MockTransport::new());
    let mut prog = Vec::new();
    kb.upload_animation(0, &[f0, f1], SYNC_INTERVAL, None, |done, total| {
        prog.push((done, total));
    })
    .unwrap();
    assert_eq!(kb.transport().sent_as_vecs(), proto.upload_slot0_no_trigger);
    assert_eq!(prog.len(), 10);
    assert_eq!(prog.last(), Some(&(10, 10)));

    let mut kb = Origins65::with_transport(MockTransport::new());
    let trig_fade = OnboardTrigger {
        kind: EffectKind::Fade,
        colors: vec![(10, 20, 30)],
        random: false,
        speed: 63,
    };
    kb.upload_animation(1, &[f0], SYNC_INTERVAL, Some(&trig_fade), |_, _| {})
        .unwrap();
    assert_eq!(kb.transport().sent_as_vecs(), proto.upload_slot1_fade);

    let mut kb = Origins65::with_transport(MockTransport::new());
    let trig_exp = OnboardTrigger {
        kind: EffectKind::Explosion,
        colors: vec![(255, 0, 128), (0, 255, 64)],
        random: true,
        speed: 200,
    };
    kb.upload_animation(2, &[f0, f1], SYNC_INTERVAL, Some(&trig_exp), |_, _| {})
        .unwrap();
    assert_eq!(
        kb.transport().sent_as_vecs(),
        proto.upload_slot2_explosion_random
    );

    let mut kb = Origins65::with_transport(MockTransport::new());
    let trig_flame = OnboardTrigger {
        kind: EffectKind::Flame,
        colors: vec![(242, 0, 0)],
        random: false,
        speed: 128,
    };
    kb.upload_animation(0, &[f1], SYNC_INTERVAL, Some(&trig_flame), |_, _| {})
        .unwrap();
    assert_eq!(kb.transport().sent_as_vecs(), proto.upload_slot0_flame);
}

#[test]
fn notifications_queued_during_ack_and_cmd_polling() {
    let mut tr = MockTransport::new();
    // Queue a key press and a brightness event BEFORE the ACK for cmd 0x05!
    tr.notifications.push_back(vec![0x05, 0x04, 26, 1]); // 'w' pressed
    tr.notifications.push_back(vec![0x05, 0x08, 128]); // brightness 50%
    tr.notifications.push_back(vec![0x05, 0xAC, 0xAC, 0xAC]); // ACK

    // Also simulate 2 busy replies before ready reply, followed by device info payload
    tr.get_queue.push_back([0u8; 64]); // busy
    tr.get_queue.push_back([0u8; 64]); // busy
    let mut ready = [0u8; 64];
    ready[1] = 0x05;
    ready[3] = 1;
    tr.get_queue.push_back(ready);

    let mut info_pkt = [0u8; 64];
    info_pkt[8] = 0xF0;
    info_pkt[9] = 0x03;
    info_pkt[12] = 0x8F;
    info_pkt[13] = 0x03;
    info_pkt[17] = 0x04;
    info_pkt[18] = 0x21;
    info_pkt[24] = 57;
    tr.get_queue.push_back(info_pkt);

    tr.auto_ack = false;

    let mut kb = Origins65::with_transport(tr);
    kb.set_timeouts(
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::ZERO,
    );

    let info = kb.device_info().unwrap();
    assert_eq!(info.vid, "03f0");
    assert_eq!(info.pid, "038f");
    assert_eq!(info.firmware, "21.04");
    assert_eq!(info.country_code, 57);

    // The two non-ACK notifications must have been preserved for read_notifications!
    let notifs = kb.read_notifications(Duration::ZERO).unwrap();
    assert_eq!(notifs.len(), 2);
    assert_eq!(
        Notification::parse(&notifs[0]),
        Some(Notification::KeyEvent {
            usage: 26,
            pressed: true
        })
    );
    assert_eq!(
        Notification::parse(&notifs[1]),
        Some(Notification::BrightnessChanged {
            level: 128,
            percent: 50
        })
    );
}

#[test]
fn layer_spec_validation_and_live_engine_triggers() {
    // Unknown effect
    let err = parse_layer_spec("rainbow").unwrap_err();
    assert!(err.contains("unknown effect 'rainbow'"));

    // Unknown key
    let err = parse_layer_spec("solid:keys=w+notakey").unwrap_err();
    assert!(err.contains("unknown keys"));
    assert!(err.contains("notakey"));

    // Unknown option
    let err = parse_layer_spec("wave:foo=bar").unwrap_err();
    assert!(err.contains("unknown option 'foo'"));

    // Speed out of range produces note warning
    let (_, warn) = parse_layer_spec("wave:speed=9999").unwrap();
    assert!(
        warn.as_deref()
            .is_some_and(|w| w.contains("outside NGENUITY's range 1000..6000 ms"))
    );

    // Onboard validation: two triggers rejected, implosion rejected
    let l1 = parse_layer_spec("fade:color=ffffff").unwrap().0;
    let l2 = parse_layer_spec("explosion:color=ff0000").unwrap().0;
    let err = prepare_onboard_save(&[l1.clone(), l2], 100.0).unwrap_err();
    assert!(err.contains("only one key-press effect"));

    let imp = parse_layer_spec("implosion:color=00ff00").unwrap().0;
    let err = prepare_onboard_save(&[imp], 100.0).unwrap_err();
    assert!(err.contains("implosion can't run onboard"));

    // Live engine: trigger overlays solid layer
    let base = parse_layer_spec("solid:color=0000ff").unwrap().0;
    let trig = parse_layer_spec("fade:color=ff0000,speed=1000").unwrap().0;
    let mut engine = Engine::new(vec![trig, base]);
    let w_key = key_by_name("w").unwrap();
    let before = engine.frame();
    assert_eq!(before[w_key.led], (0, 0, 255));
    engine.key_pressed(w_key);
    assert_eq!(engine.active.len(), 1);
    let _after = engine.frame();
}
