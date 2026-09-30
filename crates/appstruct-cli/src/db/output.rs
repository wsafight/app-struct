use super::PullMode;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) fn resolve(project: &Path, output: &Path, mode: PullMode) -> std::io::Result<PathBuf> {
    if output.is_absolute()
        || output
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(crate::transaction::invalid(
            "db pull output must be a project-relative path without parent traversal",
        ));
    }
    if !matches!(
        output.extension().and_then(|value| value.to_str()),
        Some("yaml" | "yml")
    ) {
        return Err(crate::transaction::invalid(
            "db pull output must use a .yaml or .yml extension",
        ));
    }
    let path = project.join(output);
    let mut current = project.to_path_buf();
    for component in output.parent().into_iter().flat_map(Path::components) {
        let Component::Normal(component) = component else {
            unreachable!("output components were validated")
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(crate::transaction::invalid(format!(
                    "db pull output parent `{}` is a symlink",
                    current.display()
                )));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(crate::transaction::invalid(format!(
                    "db pull output parent `{}` is not a directory",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(crate::transaction::invalid(format!(
                "db pull output `{}` must be a regular file",
                path.display()
            )));
        }
        Ok(_) if mode == PullMode::Create => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("db pull output `{}` already exists", path.display()),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && mode != PullMode::Create => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "db pull {} requires existing output `{}`",
                    if mode == PullMode::Check {
                        "--check"
                    } else {
                        "--diff"
                    },
                    path.display()
                ),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(path)
}
