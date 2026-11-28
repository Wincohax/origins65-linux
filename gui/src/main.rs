mod app;
mod driver;
mod layout;
mod model;
mod single_instance;
mod slider;
mod tray;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use gpui::{
    App, AsyncApp, Bounds, Entity, KeyBinding, QuitMode, Subscription, TitlebarOptions,
    WindowBounds, WindowHandle, WindowOptions, actions, prelude::*, px, size,
};

use crate::app::Origins65App;
use crate::single_instance::{AcquireOutcome, default_socket_path};
use crate::tray::{
    APP_ID, APP_TITLE, TrayCommand, TrayEffect, TrayHandle, TrayLifecycleState, load_window_icon,
};

actions!(origins65_gui, [Quit]);

#[derive(Debug, Default, PartialEq, Eq)]
struct CliOptions {
    background: bool,
    help: bool,
}

fn parse_cli_args<I, S>(args: I) -> Result<CliOptions, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut opts = CliOptions::default();
    for arg in args {
        match arg.as_ref() {
            "--background" | "--hidden" => opts.background = true,
            "-h" | "--help" => opts.help = true,
            other => {
                return Err(format!(
                    "Unknown option: {other}\nRun with --help for usage."
                ));
            }
        }
    }
    Ok(opts)
}

fn print_help() {
    println!(
        concat!(
            "Origins 65 Lighting — Desktop lighting controller for HyperX Alloy Origins 65 and Origins Core\n",
            "\n",
            "Usage: origins65-gui [OPTIONS]\n",
            "\n",
            "Options:\n",
            "  --background, --hidden  Start minimized to the system tray (opens window if no tray host)\n",
            "  -h, --help              Show this help message"
        )
    );
}

struct WindowController {
    app_entity: Entity<Origins65App>,
    window: Option<WindowHandle<Origins65App>>,
    lifecycle: TrayLifecycleState,
    _subscriptions: Vec<Subscription>,
}

impl WindowController {
    fn open_or_focus(&mut self, activation_token: Option<String>, cx: &mut App) {
        if let Some(handle) = self.window {
            let focused = handle
                .update(cx, |_, window, _| {
                    let used_token = activation_token
                        .as_deref()
                        .is_some_and(|tok| window.activate_with_token(tok));
                    if !used_token {
                        window.activate();
                    }
                })
                .is_ok();
            if focused {
                self.lifecycle.window_open = true;
                return;
            }
            self.window = None;
        }

        let bounds = Bounds::centered(None, size(px(1260.0), px(820.0)), cx);
        let titlebar = TitlebarOptions {
            title: Some("HyperX Alloy Origins — Lighting Studio".into()),
            ..Default::default()
        };
        let mut options = WindowOptions::default()
            .window_bounds(WindowBounds::Windowed(bounds))
            .window_min_size(size(px(940.0), px(620.0)))
            .titlebar(Some(titlebar))
            .app_id(APP_ID.to_string());
        if let Some(icon) = load_window_icon() {
            options = options.icon(icon);
        }

        let app_entity = self.app_entity.clone();
        match cx.open_window(options, |_window, _cx| app_entity) {
            Ok(handle) => {
                self.window = Some(handle);
                self.lifecycle.window_open = true;
                let _ = handle.update(cx, |_, window, _| {
                    let used_token = activation_token
                        .as_deref()
                        .is_some_and(|tok| window.activate_with_token(tok));
                    if !used_token {
                        window.activate();
                    }
                });
                cx.activate(true);
            }
            Err(err) => {
                eprintln!("origins65-gui: failed to open window: {err}");
            }
        }
    }

    fn on_window_closed(&mut self, window_id: gpui::WindowId, cx: &mut App) {
        if self.window.is_some_and(|h| h.window_id() == window_id) {
            self.window = None;
            self.lifecycle.window_open = false;
            self.app_entity
                .update(cx, |app, _| app.clear_transient_window_state());
        }
    }
}

fn main() {
    let cli = match parse_cli_args(std::env::args().skip(1)) {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    if cli.help {
        print_help();
        return;
    }

    let (cmd_tx, cmd_rx) = mpsc::channel::<TrayCommand>();

    let activation_token = std::env::var("XDG_ACTIVATION_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let single_instance = match single_instance::acquire_or_notify(
        default_socket_path(),
        !cli.background,
        activation_token,
        cmd_tx.clone(),
    ) {
        AcquireOutcome::Primary(guard) => Some(guard),
        AcquireOutcome::AlreadyRunning => return,
        AcquireOutcome::Failed(err) => {
            eprintln!("origins65-gui: single-instance disabled: {err}");
            None
        }
    };

    let tray_handle = match TrayHandle::spawn(cmd_tx) {
        Ok(handle) => Some(handle),
        Err(err) => {
            eprintln!("origins65-gui: system tray unavailable ({err}); falling back to windowed mode");
            None
        }
    };

    let initial_lifecycle = TrayLifecycleState::new(tray_handle.is_some(), false);
    let open_initial_window = initial_lifecycle.should_open_initial_window(cli.background);
    if cli.background && open_initial_window {
        eprintln!(
            "origins65-gui: --background requested, but no system tray host is available; opening window"
        );
    }

    let quit_mode = if initial_lifecycle.should_quit_on_last_window_closed() {
        QuitMode::LastWindowClosed
    } else {
        QuitMode::Explicit
    };

    let single_instance_slot = Arc::new(Mutex::new(single_instance));
    let tray_for_quit = tray_handle.clone();
    let tray_for_live = tray_handle.clone();

    gpui_platform::application()
        .with_quit_mode(quit_mode)
        .run(move |cx: &mut App| {
            cx.set_app_identity(APP_ID, APP_TITLE);
            cx.bind_keys([KeyBinding::new("ctrl-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx: &mut App| {
                cx.quit();
            });

            let on_live_changed: Option<Arc<dyn Fn(bool) + Send + Sync>> =
                tray_for_live.map(|handle| {
                    Arc::new(move |running: bool| {
                        handle.set_live_running(running);
                    }) as Arc<dyn Fn(bool) + Send + Sync>
                });

            let app_entity = cx.new(|cx| Origins65App::new(on_live_changed, cx));

            let controller = Rc::new(RefCell::new(WindowController {
                app_entity: app_entity.clone(),
                window: None,
                lifecycle: initial_lifecycle,
                _subscriptions: Vec::new(),
            }));

            let ctrl_closed = Rc::clone(&controller);
            let close_sub = cx.on_window_closed(move |cx, window_id| {
                ctrl_closed.borrow_mut().on_window_closed(window_id, cx);
            });

            let app_for_quit = app_entity.clone();
            let ipc_for_quit = Arc::clone(&single_instance_slot);
            let quit_sub = cx.on_app_quit(move |cx| {
                app_for_quit.update(cx, |app, _| app.shutdown());
                if let Some(ref tray) = tray_for_quit {
                    tray.shutdown();
                }
                if let Ok(mut guard) = ipc_for_quit.lock()
                    && let Some(mut instance) = guard.take()
                {
                    instance.shutdown();
                }
                async {}
            });

            {
                let mut ctrl = controller.borrow_mut();
                ctrl._subscriptions.push(close_sub);
                ctrl._subscriptions.push(quit_sub);
                if open_initial_window {
                    ctrl.open_or_focus(None, cx);
                }
            }

            let cmd_rx = Arc::new(Mutex::new(cmd_rx));
            let ctrl_loop = Rc::clone(&controller);
            cx.spawn(async move |cx: &mut AsyncApp| {
                loop {
                    let rx_clone = Arc::clone(&cmd_rx);
                    let next_cmd = cx
                        .background_spawn(async move {
                            let guard = rx_clone.lock().ok()?;
                            guard.recv().ok()
                        })
                        .await;
                    let Some(cmd) = next_cmd else {
                        break;
                    };

                    let mut schedule_fallback = false;
                    cx.update(|cx| {
                        let effect = ctrl_loop.borrow_mut().lifecycle.handle_command(cmd);
                        match effect {
                            TrayEffect::ShowOrFocusWindow { activation_token } => {
                                ctrl_loop.borrow_mut().open_or_focus(activation_token, cx);
                            }
                            TrayEffect::ToggleLive => {
                                let entity = ctrl_loop.borrow().app_entity.clone();
                                entity.update(cx, |app, cx| app.toggle_live_from_tray(cx));
                            }
                            TrayEffect::Quit => {
                                cx.quit();
                            }
                            TrayEffect::SetQuitOnLastWindowClosed {
                                quit_on_close,
                                ensure_window_visible_immediately,
                                schedule_window_fallback_check,
                            } => {
                                cx.set_quit_mode(if quit_on_close {
                                    QuitMode::LastWindowClosed
                                } else {
                                    QuitMode::Explicit
                                });
                                if ensure_window_visible_immediately {
                                    ctrl_loop.borrow_mut().open_or_focus(None, cx);
                                }
                                schedule_fallback = schedule_window_fallback_check;
                            }
                        }
                    });

                    if schedule_fallback {
                        let ctrl_fallback = Rc::clone(&ctrl_loop);
                        cx.spawn(async move |cx: &mut AsyncApp| {
                            cx.background_executor().timer(Duration::from_secs(2)).await;
                            cx.update(|cx| {
                                let need_window = {
                                    let ctrl = ctrl_fallback.borrow();
                                    !ctrl.lifecycle.tray_online && !ctrl.lifecycle.window_open
                                };
                                if need_window {
                                    ctrl_fallback.borrow_mut().open_or_focus(None, cx);
                                }
                            });
                        })
                        .detach();
                    }
                }
            })
            .detach();
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_background_hidden_and_help_flags() {
        assert!(parse_cli_args(["--background"]).unwrap().background);
        assert!(parse_cli_args(["--hidden"]).unwrap().background);
        assert!(parse_cli_args(["--help"]).unwrap().help);
        assert!(parse_cli_args(["-h"]).unwrap().help);
        assert_eq!(
            parse_cli_args(Vec::<&str>::new()).unwrap(),
            CliOptions::default()
        );
        assert!(parse_cli_args(["--bogus"]).is_err());
    }
}
