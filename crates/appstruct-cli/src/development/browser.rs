use super::process::DevProcesses;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::Duration;

pub(super) fn wait_for_web(
    processes: &mut DevProcesses,
    web_port: u16,
    stopping: &AtomicBool,
) -> io::Result<()> {
    let address = SocketAddr::from(([127, 0, 0, 1], web_port));
    for _ in 0..120 {
        super::check_stopping(stopping)?;
        if let Some(failure) = processes.failure()? {
            return Err(io::Error::other(failure));
        }
        if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "generated Web application did not become ready within 30 seconds",
    ))
}

pub(super) fn launch(url: &str) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status()?;
    #[cfg(target_os = "windows")]
    let status = Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = Command::new("xdg-open").arg(url).status()?;
    #[cfg(not(any(unix, target_os = "windows")))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening a browser is not supported on this platform",
    ));
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "browser launcher exited with {status}"
        )))
    }
}
