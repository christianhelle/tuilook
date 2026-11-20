//! Opening web addresses in the user's default browser.

use std::io;
#[cfg(any(target_os = "windows", target_os = "macos", unix))]
use std::process::{Command, Stdio};

/// Asks the default browser to open `url`, without waiting for it.
#[cfg(any(target_os = "windows", target_os = "macos", unix))]
pub fn open(url: &str) -> io::Result<()> {
    let mut child = command(url).spawn()?;
    // The launcher may outlive the browser it starts; wait for it off the UI
    // thread so it leaves no zombie behind.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
pub fn open(_url: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no browser launcher for this system",
    ))
}

#[cfg(target_os = "windows")]
fn command(url: &str) -> Command {
    let mut command = Command::new("rundll32");
    command.args(["url.dll,FileProtocolHandler", url]);
    quiet(command)
}

#[cfg(target_os = "macos")]
fn command(url: &str) -> Command {
    let mut command = Command::new("open");
    command.arg(url);
    quiet(command)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn command(url: &str) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    quiet(command)
}

/// Keeps the launcher's output off the terminal, which the UI is drawing on.
#[cfg(any(target_os = "windows", target_os = "macos", unix))]
fn quiet(mut command: Command) -> Command {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
