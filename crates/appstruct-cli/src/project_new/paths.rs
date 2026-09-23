use std::io;
use std::path::{Component, Path};

pub(super) fn validate_relative_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid(format!(
            "unsafe template path `{}`",
            path.display()
        )));
    }
    Ok(())
}

pub(super) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(not(windows))]
pub(super) fn cd_command(destination: &Path) -> String {
    format!(
        "cd '{}'",
        destination.display().to_string().replace('\'', "'\\''")
    )
}

#[cfg(windows)]
pub(super) fn cd_command(destination: &Path) -> String {
    format!(
        "Set-Location -LiteralPath '{}'",
        destination.display().to_string().replace('\'', "''")
    )
}
