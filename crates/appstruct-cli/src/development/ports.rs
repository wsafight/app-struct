use crate::environment::ProjectEnvironment;
use std::io;
use std::path::Path;

pub(super) fn resolve(
    project: &Path,
    api_override: Option<u16>,
    web_override: Option<u16>,
) -> io::Result<(u16, u16)> {
    if api_override == Some(0) || web_override == Some(0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "API and web ports must be non-zero",
        ));
    }
    let environment = ProjectEnvironment::load(project)?;
    let api_port = port(&environment, "APPSTRUCT_API_PORT", api_override, 3000)?;
    let web_port = port(&environment, "APPSTRUCT_WEB_PORT", web_override, 5173)?;
    if api_port == web_port {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "API and web ports must be different",
        ));
    }
    Ok((api_port, web_port))
}

fn port(
    environment: &ProjectEnvironment,
    name: &str,
    override_value: Option<u16>,
    default: u16,
) -> io::Result<u16> {
    if let Some(value) = override_value {
        return Ok(value);
    }
    let Some(configured) = environment.get(name) else {
        return Ok(default);
    };
    configured
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{name} must be a port from 1 to 65535"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn project_ports_are_defaults_and_flags_override_them() {
        let project = tempfile::tempdir().unwrap();
        fs::write(
            project.path().join(".env"),
            "APPSTRUCT_API_PORT=3100\nAPPSTRUCT_WEB_PORT=5200\n",
        )
        .unwrap();
        assert_eq!(resolve(project.path(), None, None).unwrap(), (3100, 5200));
        assert_eq!(
            resolve(project.path(), Some(3200), None).unwrap(),
            (3200, 5200)
        );
    }

    #[test]
    fn invalid_ports_fail_before_startup() {
        let project = tempfile::tempdir().unwrap();
        assert!(resolve(project.path(), Some(0), None).is_err());
        assert!(resolve(project.path(), Some(5173), None).is_err());
        fs::write(project.path().join(".env"), "APPSTRUCT_API_PORT=invalid\n").unwrap();
        assert!(resolve(project.path(), None, None).is_err());
    }
}
