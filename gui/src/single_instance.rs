use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread;
use std::time::Duration;

use crate::tray::TrayCommand;

const SOCKET_FILENAME: &str = "origins65-gui.sock";
const MAX_UNIX_SOCKET_PATH_LEN: usize = 100;
const IO_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceMessage {
    Show { activation_token: Option<String> },
    Ping,
}

impl InstanceMessage {
    pub fn encode(&self) -> String {
        match self {
            InstanceMessage::Show {
                activation_token: Some(tok),
            } if !tok.is_empty() && !tok.contains('\n') && !tok.contains('\r') => {
                format!("SHOW {tok}\n")
            }
            InstanceMessage::Show { .. } => "SHOW\n".into(),
            InstanceMessage::Ping => "PING\n".into(),
        }
    }

    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed == "PING" {
            return Some(InstanceMessage::Ping);
        }
        if trimmed == "SHOW" {
            return Some(InstanceMessage::Show {
                activation_token: None,
            });
        }
        if let Some(rest) = trimmed.strip_prefix("SHOW ") {
            let tok = rest.trim();
            return Some(InstanceMessage::Show {
                activation_token: (!tok.is_empty()).then(|| tok.to_string()),
            });
        }
        None
    }
}

pub fn default_socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR")
        && !dir.is_empty()
    {
        let candidate = PathBuf::from(dir).join(SOCKET_FILENAME);
        if candidate.as_os_str().len() <= MAX_UNIX_SOCKET_PATH_LEN {
            return candidate;
        }
    }
    let uid = fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0);
    let fallback_name = format!("origins65-gui-{uid}.sock");
    let candidate = std::env::temp_dir().join(&fallback_name);
    if candidate.as_os_str().len() <= MAX_UNIX_SOCKET_PATH_LEN {
        return candidate;
    }
    PathBuf::from("/tmp").join(fallback_name)
}

pub enum AcquireOutcome {
    Primary(SingleInstanceGuard),
    AlreadyRunning,
    Failed(String),
}

pub struct SingleInstanceGuard {
    path: PathBuf,
    stop_flag: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl SingleInstanceGuard {
    pub fn shutdown(&mut self) {
        if self.stop_flag.swap(true, Ordering::SeqCst) {
            return;
        }
        // Poke the blocking accept() call so the listener thread exits promptly.
        let _ = UnixStream::connect(&self.path);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn try_notify_existing(path: &Path, message: &InstanceMessage) -> Result<(), std::io::Error> {
    let mut stream = UnixStream::connect(path)?;
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    stream.write_all(message.encode().as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    let n = reader.read_line(&mut reply)?;
    if n > 0 && reply.trim() == "OK" {
        Ok(())
    } else {
        Err(std::io::Error::new(
            ErrorKind::UnexpectedEof,
            "peer did not acknowledge single-instance message",
        ))
    }
}

/// Attempts to claim the single-instance Unix socket.
///
/// - If another instance is alive, forwards `SHOW` (or `PING` when `show_existing == false`)
///   and returns [`AcquireOutcome::AlreadyRunning`].
/// - If a stale socket file exists on disk, removes it and binds cleanly.
pub fn acquire_or_notify(
    path: PathBuf,
    show_existing: bool,
    activation_token: Option<String>,
    cmd_tx: Sender<TrayCommand>,
) -> AcquireOutcome {
    let message = if show_existing {
        InstanceMessage::Show { activation_token }
    } else {
        InstanceMessage::Ping
    };

    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            if try_notify_existing(&path, &message).is_ok() {
                return AcquireOutcome::AlreadyRunning;
            }
            // Stale socket from an unclean exit — remove and rebind.
            let _ = fs::remove_file(&path);
            match UnixListener::bind(&path) {
                Ok(l) => l,
                Err(retry_err) if retry_err.kind() == ErrorKind::AddrInUse => {
                    // Lost a tight startup race with another fresh launch.
                    if try_notify_existing(&path, &message).is_ok() {
                        return AcquireOutcome::AlreadyRunning;
                    }
                    return AcquireOutcome::Failed(format!(
                        "could not bind single-instance socket {}: {retry_err}",
                        path.display()
                    ));
                }
                Err(retry_err) => {
                    return AcquireOutcome::Failed(format!(
                        "could not bind single-instance socket {}: {retry_err}",
                        path.display()
                    ));
                }
            }
        }
        Err(err) => {
            return AcquireOutcome::Failed(format!(
                "could not bind single-instance socket {}: {err}",
                path.display()
            ));
        }
    };

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_worker = Arc::clone(&stop_flag);
    let worker = thread::Builder::new()
        .name("origins65-ipc".into())
        .spawn(move || {
            for conn in listener.incoming() {
                if stop_worker.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = conn else {
                    continue;
                };
                handle_client(stream, &cmd_tx);
            }
        });

    match worker {
        Ok(handle) => AcquireOutcome::Primary(SingleInstanceGuard {
            path,
            stop_flag,
            worker: Some(handle),
        }),
        Err(err) => {
            let _ = fs::remove_file(&path);
            AcquireOutcome::Failed(format!("failed to spawn single-instance thread: {err}"))
        }
    }
}

fn handle_client(stream: UnixStream, cmd_tx: &Sender<TrayCommand>) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    if let Some(msg) = InstanceMessage::parse(&line) {
        if let InstanceMessage::Show { activation_token } = msg {
            let _ = cmd_tx.send(TrayCommand::ShowWindow { activation_token });
        }
        let _ = writer.write_all(b"OK\n");
        let _ = writer.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_socket(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let filename = format!("o65-{name}-{}-{}.sock", std::process::id(), nanos % 1_000_000);
        let dir = std::env::temp_dir();
        let candidate = dir.join(&filename);
        if candidate.as_os_str().len() <= MAX_UNIX_SOCKET_PATH_LEN {
            candidate
        } else {
            PathBuf::from("/tmp").join(filename)
        }
    }

    #[test]
    fn instance_message_encode_and_parse_round_trip() {
        let show = InstanceMessage::Show {
            activation_token: None,
        };
        assert_eq!(show.encode(), "SHOW\n");
        assert_eq!(InstanceMessage::parse(&show.encode()), Some(show));

        let show_tok = InstanceMessage::Show {
            activation_token: Some("xdg-token-42".into()),
        };
        assert_eq!(show_tok.encode(), "SHOW xdg-token-42\n");
        assert_eq!(InstanceMessage::parse(&show_tok.encode()), Some(show_tok));

        // Newlines in tokens are sanitized out so framing cannot be broken.
        let bad_tok = InstanceMessage::Show {
            activation_token: Some("bad\ntoken".into()),
        };
        assert_eq!(bad_tok.encode(), "SHOW\n");

        assert_eq!(InstanceMessage::Ping.encode(), "PING\n");
        assert_eq!(InstanceMessage::parse("PING\n"), Some(InstanceMessage::Ping));
        assert_eq!(InstanceMessage::parse("UNKNOWN\n"), None);
    }

    #[test]
    fn second_instance_notifies_primary_and_cleans_up_on_drop() {
        let sock = unique_test_socket("notify");
        let (tx1, rx1) = mpsc::channel();
        let guard = match acquire_or_notify(sock.clone(), true, None, tx1) {
            AcquireOutcome::Primary(g) => g,
            _ => panic!("expected primary instance"),
        };

        // Normal second launch asks primary to show its window with token.
        let (tx2, _rx2) = mpsc::channel();
        assert!(matches!(
            acquire_or_notify(sock.clone(), true, Some("tok-abc".into()), tx2),
            AcquireOutcome::AlreadyRunning
        ));
        let received = rx1.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            received,
            TrayCommand::ShowWindow {
                activation_token: Some("tok-abc".into())
            }
        );

        // Background second launch pings cleanly without requesting ShowWindow.
        let (tx3, _rx3) = mpsc::channel();
        assert!(matches!(
            acquire_or_notify(sock.clone(), false, None, tx3),
            AcquireOutcome::AlreadyRunning
        ));
        assert!(rx1.recv_timeout(Duration::from_millis(100)).is_err());

        drop(guard);
        assert!(
            !sock.exists(),
            "socket file should be removed when guard drops"
        );
    }

    #[test]
    fn stale_socket_is_replaced_cleanly() {
        let sock = unique_test_socket("stale");
        // Bind and drop a raw UnixListener, leaving a stale socket file behind.
        {
            let _stale = UnixListener::bind(&sock).unwrap();
        }
        assert!(sock.exists());

        let (tx1, rx1) = mpsc::channel();
        let guard = match acquire_or_notify(sock.clone(), true, None, tx1) {
            AcquireOutcome::Primary(g) => g,
            _ => panic!("expected primary instance to recover from stale socket"),
        };

        let (tx2, _rx2) = mpsc::channel();
        assert!(matches!(
            acquire_or_notify(sock.clone(), true, None, tx2),
            AcquireOutcome::AlreadyRunning
        ));
        assert_eq!(
            rx1.recv_timeout(Duration::from_secs(2)).unwrap(),
            TrayCommand::ShowWindow {
                activation_token: None
            }
        );

        drop(guard);
        assert!(!sock.exists());
    }
}
