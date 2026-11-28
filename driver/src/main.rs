use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use origins65::device::{Notification, SLOTS, prepare_onboard_save};
use origins65::effects::{EffectKind, Engine, parse_layer_spec_for, scale_frame};
use origins65::keyboard::{detect_model, open_keyboard};
use origins65::layout::NUM_LEDS;
use origins65::model::Model;

const ABOUT: &str = "\
HyperX Alloy Origins 65 (03f0:038f) and Alloy Origins Core TKL (03f0:098f, 0951:16e6) lighting
driver for Linux. The connected keyboard is detected automatically.

Effects are written as layer specs, top layer first:
    NAME[:key=value,...]
      speed=MS           cycle length (each effect has NGENUITY's range, see `effects`)
      color=RRGGBB|name  single color, or color=random
      colors=C1/C2/...   gradient / color list (wave, cycle, swipe)
      keys=w+a+s+d       restrict the layer to some keys (see `keys`)
      opacity=0..100     layer opacity
      angle=DEG          wave direction (default 180)
      direction=D        swipe: left|right|up|down

Examples:
    origins65 live wave
    origins65 live explosion:color=random wave:speed=4000
    origins65 live solid:color=ff0000,keys=w+a+s+d breathing:color=0040ff
    origins65 save --slot 0 wave:speed=3000 fade:color=ffffff
    origins65 brightness 50";

#[derive(Parser)]
#[command(name = "origins65", about = ABOUT, verbatim_doc_comment)]
struct Cli {
    /// Keyboard model (default: detect the connected one)
    #[arg(long, global = true, value_parser = parse_model)]
    model: Option<Model>,
    #[command(subcommand)]
    cmd: Cmd,
}

fn parse_model(s: &str) -> Result<Model, String> {
    Model::from_id(s).ok_or_else(|| {
        let ids: Vec<&str> = Model::ALL.iter().map(|m| m.id()).collect();
        format!("expected one of: {}", ids.join(", "))
    })
}

#[derive(Subcommand)]
enum Cmd {
    /// Device, firmware and profile information
    Info {
        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Set hardware brightness (0-100)
    Brightness {
        #[arg(allow_hyphen_values = true)]
        percent: i32,
    },
    /// Activate onboard slot 0-2
    Slot {
        #[arg(value_parser = clap::value_parser!(u8).range(0..SLOTS as i64))]
        slot: u8,
    },
    /// Stream effects (key-press effects react to typing)
    Live {
        #[arg(required = true)]
        layers: Vec<String>,
        #[arg(long, default_value_t = 30.0)]
        fps: f64,
        /// Software brightness 0-100
        #[arg(long, default_value_t = 100.0)]
        brightness: f64,
        #[arg(long, default_value_t = 0.0)]
        duration: f64,
    },
    /// Store effects in onboard memory (runs without this tool)
    Save {
        #[arg(required = true)]
        layers: Vec<String>,
        #[arg(long, required = true, value_parser = clap::value_parser!(u8).range(0..SLOTS as i64))]
        slot: u8,
        #[arg(long, default_value_t = 100.0)]
        brightness: f64,
        #[arg(long)]
        no_activate: bool,
    },
    /// Light every key in turn
    Identify {
        #[arg(long, default_value_t = 0.5)]
        step: f64,
    },
    /// List key names
    Keys,
    /// List effects
    Effects,
}

fn parse_layers_cli(
    specs: &[String],
    model: Model,
) -> Result<Vec<origins65::effects::Layer>, String> {
    let mut out = Vec::with_capacity(specs.len());
    for spec in specs {
        let (layer, warning) = parse_layer_spec_for(spec, model)?;
        if let Some(note) = warning {
            eprintln!("{note}");
        }
        out.push(layer);
    }
    Ok(out)
}

fn install_stop_handler() -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let _ = ctrlc::set_handler(move || {
        flag.store(true, Ordering::SeqCst);
    });
    stop
}

fn run(cli: Cli) -> Result<(), String> {
    let offline_model = || cli.model.or_else(detect_model).unwrap_or_default();
    match cli.cmd {
        Cmd::Keys => {
            let model = offline_model();
            let line = model.keys().iter().map(|k| k.name).collect::<Vec<_>>().join(" ");
            println!("{line}");
            return Ok(());
        }
        Cmd::Effects => {
            let model = offline_model();
            for kind in EffectKind::ALL {
                let kind_str = if kind.is_triggered() {
                    if kind.onboard_trigger_code().is_some() {
                        "key-press (onboard ok)"
                    } else {
                        "key-press (live only)"
                    }
                } else {
                    "looped"
                };
                let (min, max, default) = model.speed_range(kind);
                println!(
                    "{:<10} {:<22} speed {min}-{max} ms, default {default}",
                    kind.name(),
                    kind_str,
                );
            }
            return Ok(());
        }
        _ => {}
    }

    match cli.cmd {
        Cmd::Info { json } => {
            let mut kb = open_keyboard(cli.model).map_err(|e| e.to_string())?;
            if json {
                let full = kb.read_full_info().map_err(|e| e.to_string())?;
                let text = to_python_json(&full)?;
                println!("{text}");
            } else {
                let info = kb.device_info().map_err(|e| e.to_string())?;
                let current = kb.read_profile(None).map_err(|e| e.to_string())?;
                println!(
                    "{}  {}:{}  firmware {}  country code {}",
                    kb.model().name(),
                    info.vid,
                    info.pid,
                    info.firmware,
                    info.country_code
                );
                println!("current profile: {}", current.describe());
                for s in 0..SLOTS {
                    match kb.read_profile(Some(s)) {
                        Ok(prof) => println!("slot {s} profile:   {}", prof.describe()),
                        Err(e) => println!("slot {s} profile:   unreadable ({e})"),
                    }
                }
            }
        }

        Cmd::Brightness { percent } => {
            let mut kb = open_keyboard(cli.model).map_err(|e| e.to_string())?;
            let prof = kb.set_brightness(percent).map_err(|e| e.to_string())?;
            println!("brightness set: {}", prof.describe());
        }

        Cmd::Slot { slot } => {
            let mut kb = open_keyboard(cli.model).map_err(|e| e.to_string())?;
            kb.select_slot(slot, false).map_err(|e| e.to_string())?;
            let active = kb.active_slot().map_err(|e| e.to_string())?;
            println!("active onboard slot: {active}");
        }

        Cmd::Live {
            layers: specs,
            fps,
            brightness,
            duration,
        } => {
            let mut kb = open_keyboard(cli.model).map_err(|e| e.to_string())?;
            let model = kb.model();
            let layers = parse_layers_cli(&specs, model)?;
            let mut engine = Engine::new(layers);
            let stop = install_stop_handler();
            let period = if fps > 0.0 {
                Duration::from_secs_f64(1.0 / fps)
            } else {
                Duration::from_millis(33)
            };
            let t0 = Instant::now();
            println!(
                "streaming (Ctrl+C to stop; the keyboard returns to its onboard lighting afterwards)"
            );
            let _ = io::stdout().flush();

            while !stop.load(Ordering::SeqCst) {
                let start = Instant::now();
                if duration > 0.0 && start.duration_since(t0).as_secs_f64() >= duration {
                    break;
                }
                for event in kb.poll_events(Duration::ZERO).map_err(|e| e.to_string())? {
                    match event {
                        Notification::KeyEvent {
                            usage,
                            pressed: true,
                        } => {
                            if let Some(key) = model.key_by_usage(usage) {
                                engine.key_pressed(key);
                            }
                        }
                        Notification::BrightnessChanged { percent, .. } => {
                            println!("hardware brightness changed to {percent}%");
                            let _ = io::stdout().flush();
                        }
                        _ => {}
                    }
                }
                let frame = scale_frame(&engine.frame(), brightness / 100.0);
                kb.send_direct_frame(&frame).map_err(|e| e.to_string())?;
                let elapsed = start.elapsed();
                if elapsed < period {
                    thread::sleep(period - elapsed);
                }
            }
        }

        Cmd::Save {
            layers: specs,
            slot,
            brightness,
            no_activate,
        } => {
            let mut kb = open_keyboard(cli.model).map_err(|e| e.to_string())?;
            let model = kb.model();
            let layers = parse_layers_cli(&specs, model)?;
            let prepared = prepare_onboard_save(&layers, brightness, model)?;

            let fps = model.sync_fps();
            let secs = (prepared.frames.len() as f64) / (fps as f64);
            let trig_suffix = match &prepared.trigger {
                Some(t) => format!(", onboard key-press effect: {}", t.kind.name()),
                None => String::new(),
            };
            println!(
                "rendered {} frames ({secs:.1} s loop at {fps} fps){trig_suffix}",
                prepared.frames.len()
            );
            let _ = io::stdout().flush();

            let profile_note = kb
                .save_prepared(slot, &prepared, !no_activate, &mut |done, total| {
                    let pct = done * 100 / total;
                    print!("\ruploading to slot {slot}: {pct:3}%");
                    let _ = io::stdout().flush();
                })
                .map_err(|e| e.to_string())?;
            println!();
            if let Some(note) = profile_note {
                eprintln!("{note}");
            }
            println!(
                "saved to onboard slot {slot}{}",
                if no_activate { "" } else { " and activated" }
            );
        }

        Cmd::Identify { step } => {
            let mut kb = open_keyboard(cli.model).map_err(|e| e.to_string())?;
            let mut order: Vec<_> = kb.model().keys().iter().collect();
            order.sort_by_key(|k| (k.row, k.col));
            let stop = install_stop_handler();
            let step_dur = Duration::from_secs_f64(step.max(0.0));
            for k in order {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                println!("LED {:2}  row {} col {:2}  {}", k.led, k.row, k.col, k.name);
                let _ = io::stdout().flush();
                let end = Instant::now() + step_dur;
                while Instant::now() < end && !stop.load(Ordering::SeqCst) {
                    let mut frame = [(0u8, 0u8, 25u8); NUM_LEDS];
                    frame[k.led] = (255, 255, 255);
                    kb.send_direct_frame(&frame).map_err(|e| e.to_string())?;
                    thread::sleep(Duration::from_millis(30));
                }
            }
        }

        Cmd::Keys | Cmd::Effects => unreachable!(),
    }

    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

/// Serialize like Python's `json.dumps` defaults (`", "` and `": "` separators), so `info --json`
/// is byte-identical to `tools/origins65.py info --json`.
fn to_python_json<T: serde::Serialize>(value: &T) -> Result<String, String> {
    struct PyFormatter;
    impl serde_json::ser::Formatter for PyFormatter {
        fn begin_array_value<W: ?Sized + std::io::Write>(&mut self, w: &mut W, first: bool) -> std::io::Result<()> {
            if first { Ok(()) } else { w.write_all(b", ") }
        }
        fn begin_object_key<W: ?Sized + std::io::Write>(&mut self, w: &mut W, first: bool) -> std::io::Result<()> {
            if first { Ok(()) } else { w.write_all(b", ") }
        }
        fn begin_object_value<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
            w.write_all(b": ")
        }
    }
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, PyFormatter);
    value.serialize(&mut ser).map_err(|e| e.to_string())?;
    String::from_utf8(out).map_err(|e| e.to_string())
}
