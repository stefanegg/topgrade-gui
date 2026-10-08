//! Runs topgrade inside a PTY and streams its raw output to the frontend,
//! interleaved with parser events on the same channel so their order relative
//! to the output is preserved.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::parser::Parser;

/// Flags the frontend may toggle; anything else is rejected.
const ALLOWED_FLAGS: &[&str] = &["--cleanup", "-y", "--yes", "--dry-run", "-n"];

/// Grace period between Ctrl-C and a hard kill on Stop.
const STOP_GRACE: Duration = Duration::from_secs(5);

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        /// Raw output is saved here; `None` if the file couldn't be created.
        log_path: Option<String>,
    },
    Exited { code: Option<u32> },
    Error { message: String },
}

struct ActiveRun {
    id: u64,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
pub struct Runner {
    active: Arc<Mutex<Option<ActiveRun>>>,
    next_id: Mutex<u64>,
}

impl Runner {
    pub fn start(
        &self,
        flags: Vec<String>,
        cols: u16,
        rows: u16,
        log_dir: &Path,
        stream: Channel<InvokeResponseBody>,
    ) -> Result<(), String> {
        if let Some(bad) = flags.iter().find(|f| !ALLOWED_FLAGS.contains(&f.as_str())) {
            return Err(format!("flag not allowed: {bad}"));
        }

        let mut active = self.active.lock().unwrap();
        if active.is_some() {
            return Err("topgrade is already running".into());
        }

        let pair = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;

        let mut child = pair
            .slave
            .spawn_command(build_command(&flags))
            .map_err(|e| e.to_string())?;
        // Drop our slave handle so the reader sees EOF once the child exits.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let killer = child.clone_killer();

        let id = {
            let mut next = self.next_id.lock().unwrap();
            *next += 1;
            *next
        };

        // Logging is best-effort: a run must not fail because of it.
        let log_path = log_file_path(log_dir, &flags);
        let mut log = fs::create_dir_all(log_dir).and_then(|_| File::create(&log_path)).ok();
        send_event(
            &stream,
            &RunEvent::Started { log_path: log.as_ref().map(|_| log_path.display().to_string()) },
        );

        let output = stream.clone();
        let reader_thread = thread::spawn(move || {
            let mut parser = Parser::default();
            let mut buf = [0u8; 8192];
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                if let Some(file) = log.as_mut() {
                    if file.write_all(&buf[..n]).is_err() {
                        log = None;
                    }
                }
                // Split the chunk at each event so the frontend sees the
                // event exactly after the output that triggered it.
                let mut sent = 0;
                for (offset, event) in parser.feed(&buf[..n]) {
                    if offset > sent {
                        send_bytes(&output, &buf[sent..offset]);
                        sent = offset;
                    }
                    send_event(&output, &event);
                }
                if sent < n {
                    send_bytes(&output, &buf[sent..n]);
                }
            }
        });

        let active_ref = Arc::clone(&self.active);
        thread::spawn(move || {
            let status = child.wait();
            let _ = reader_thread.join();
            {
                let mut active = active_ref.lock().unwrap();
                if active.as_ref().is_some_and(|run| run.id == id) {
                    *active = None;
                }
            }
            let event = match status {
                Ok(status) => RunEvent::Exited { code: Some(status.exit_code()) },
                Err(e) => RunEvent::Error { message: e.to_string() },
            };
            send_event(&stream, &event);
        });

        *active = Some(ActiveRun { id, master: pair.master, writer, killer });
        Ok(())
    }

    pub fn write(&self, data: &str) -> Result<(), String> {
        let mut active = self.active.lock().unwrap();
        let run = active.as_mut().ok_or("not running")?;
        run.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        run.writer.flush().map_err(|e| e.to_string())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let active = self.active.lock().unwrap();
        if let Some(run) = active.as_ref() {
            run.master
                .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Sends Ctrl-C through the terminal (SIGINT to the foreground process
    /// group), then kills the child if it is still running after a grace period.
    pub fn stop(&self) -> Result<(), String> {
        let id = {
            let mut active = self.active.lock().unwrap();
            let run = active.as_mut().ok_or("not running")?;
            let _ = run.writer.write_all(b"\x03");
            let _ = run.writer.flush();
            run.id
        };
        let active_ref = Arc::clone(&self.active);
        thread::spawn(move || {
            thread::sleep(STOP_GRACE);
            let mut active = active_ref.lock().unwrap();
            if let Some(run) = active.as_mut().filter(|run| run.id == id) {
                let _ = run.killer.kill();
            }
        });
        Ok(())
    }

    /// Hard-kills any running child; used when the app exits.
    pub fn kill(&self) {
        if let Some(run) = self.active.lock().unwrap().as_mut() {
            let _ = run.killer.kill();
        }
    }
}

/// `<dir>/2026-10-08T21-30-00-macos[-dry-run].log`: raw PTY bytes, usable as
/// parser fixtures and for replay.
fn log_file_path(dir: &Path, flags: &[String]) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S");
    let dry = if flags.iter().any(|f| f == "--dry-run" || f == "-n") { "-dry-run" } else { "" };
    dir.join(format!("{stamp}-{}{dry}.log", std::env::consts::OS))
}

fn send_bytes(stream: &Channel<InvokeResponseBody>, bytes: &[u8]) {
    let _ = stream.send(InvokeResponseBody::Raw(bytes.to_vec()));
}

fn send_event(stream: &Channel<InvokeResponseBody>, event: &impl Serialize) {
    if let Ok(json) = serde_json::to_string(event) {
        let _ = stream.send(InvokeResponseBody::Json(json));
    }
}

/// Runs topgrade through the user's login shell so GUI-launched apps get the
/// same PATH as a terminal (macOS GUI apps start with a minimal PATH).
fn build_command(flags: &[String]) -> CommandBuilder {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    // Topgrade output is localized; force English so the parser can match it.
    // Set after profile loading so the user's shell config cannot override it.
    let script = format!(
        "export LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8; exec topgrade {}",
        flags.join(" ")
    );
    let mut cmd = CommandBuilder::new(shell);
    cmd.args(["-l", "-c", &script]);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(home);
    }
    cmd
}
