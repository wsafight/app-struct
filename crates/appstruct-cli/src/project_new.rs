use clap::ValueEnum;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

const PROJECT_NAME_MARKER: &str = "__APPSTRUCT_PROJECT_NAME__";

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum ProjectTemplate {
    Minimal,
    Dashboard,
    Saas,
}

struct TemplateFile {
    path: &'static str,
    content: &'static str,
}

pub(crate) fn run(parent: &Path, name: &str, template: ProjectTemplate) -> ExitCode {
    match create(parent, name, template) {
        Ok(destination) => {
            if crate::report::is_json() {
                crate::report::success(&serde_json::json!({
                    "command": "new",
                    "name": name,
                    "template": template.name(),
                    "path": destination,
                }));
            } else {
                println!("Created AppStruct project at {}", destination.display());
                println!("Next: cd {name} && appstruct dev");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            let exit = if matches!(
                error.kind(),
                io::ErrorKind::InvalidInput | io::ErrorKind::AlreadyExists
            ) {
                crate::report::ExitClass::Validation
            } else {
                crate::report::ExitClass::Environment
            };
            crate::report::fail(
                "AS6002",
                crate::report::ErrorCategory::Project,
                format!("cannot create project: {error}"),
                exit,
            )
        }
    }
}

fn create(parent: &Path, name: &str, template: ProjectTemplate) -> io::Result<PathBuf> {
    validate_name(name)?;
    if !parent.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("parent directory `{}` does not exist", parent.display()),
        ));
    }
    let destination = parent.join(name);
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("destination `{}` already exists", destination.display()),
        ));
    }
    let staging = parent.join(format!(".{name}.appstruct-new-staging"));
    if staging.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("staging directory `{}` already exists", staging.display()),
        ));
    }
    if let Err(error) = write_template(&staging, name, template, template_files(template)) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if destination.exists() {
        let _ = fs::remove_dir_all(&staging);
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "destination `{}` appeared during creation",
                destination.display()
            ),
        ));
    }
    if let Err(error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    Ok(destination)
}

fn write_template(
    root: &Path,
    name: &str,
    template: ProjectTemplate,
    files: &[TemplateFile],
) -> io::Result<()> {
    fs::create_dir(root)?;
    for file in files {
        let relative = Path::new(file.path);
        validate_relative_path(relative)?;
        let destination = root.join(relative);
        let parent = destination
            .parent()
            .ok_or_else(|| invalid("template file has no parent"))?;
        fs::create_dir_all(parent)?;
        fs::write(destination, file.content.replace(PROJECT_NAME_MARKER, name))?;
    }
    let lock = appstruct_compiler::project_lock(template.name(), template.preset())
        .ok_or_else(|| invalid("template selects an unsupported preset"))?;
    fs::write(root.join("appstruct.lock"), lock)?;
    Ok(())
}

impl ProjectTemplate {
    const fn name(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Dashboard => "dashboard",
            Self::Saas => "saas",
        }
    }

    const fn preset(self) -> Option<(&'static str, u64)> {
        match self {
            Self::Saas => Some(("appstruct/saas", 1)),
            Self::Minimal | Self::Dashboard => None,
        }
    }
}

fn validate_name(name: &str) -> io::Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && name
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project name must be 1-64 lowercase ASCII letters, digits, or hyphens, starting with a letter",
        ))
    }
}

fn validate_relative_path(path: &Path) -> io::Result<()> {
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

fn template_files(template: ProjectTemplate) -> &'static [TemplateFile] {
    match template {
        ProjectTemplate::Minimal => MINIMAL_FILES,
        ProjectTemplate::Dashboard => DASHBOARD_FILES,
        ProjectTemplate::Saas => SAAS_FILES,
    }
}

const MINIMAL_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "app/backend/Cargo.toml",
        content: include_str!("../templates/common/app-backend.Cargo.toml"),
    },
    TemplateFile {
        path: "app/backend/src/lib.rs",
        content: include_str!("../templates/common/app-backend.lib.rs"),
    },
    TemplateFile {
        path: ".gitignore",
        content: include_str!("../templates/common/gitignore"),
    },
    TemplateFile {
        path: ".env.example",
        content: include_str!("../templates/minimal/env.example"),
    },
    TemplateFile {
        path: "README.md",
        content: include_str!("../templates/minimal/README.md"),
    },
    TemplateFile {
        path: "appstruct.yaml",
        content: include_str!("../templates/minimal/appstruct.yaml"),
    },
    TemplateFile {
        path: "rust-toolchain.toml",
        content: include_str!("../templates/common/rust-toolchain.toml"),
    },
    TemplateFile {
        path: "spec/main.yaml",
        content: include_str!("../templates/minimal/spec/main.yaml"),
    },
];

const DASHBOARD_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "app/backend/Cargo.toml",
        content: include_str!("../templates/common/app-backend.Cargo.toml"),
    },
    TemplateFile {
        path: "app/backend/src/lib.rs",
        content: include_str!("../templates/common/app-backend.lib.rs"),
    },
    TemplateFile {
        path: ".gitignore",
        content: include_str!("../templates/common/gitignore"),
    },
    TemplateFile {
        path: ".env.example",
        content: include_str!("../templates/dashboard/env.example"),
    },
    TemplateFile {
        path: "README.md",
        content: include_str!("../templates/dashboard/README.md"),
    },
    TemplateFile {
        path: "appstruct.yaml",
        content: include_str!("../templates/dashboard/appstruct.yaml"),
    },
    TemplateFile {
        path: "compose.yaml",
        content: include_str!("../templates/dashboard/compose.yaml"),
    },
    TemplateFile {
        path: "rust-toolchain.toml",
        content: include_str!("../templates/common/rust-toolchain.toml"),
    },
    TemplateFile {
        path: "spec/identity.yaml",
        content: include_str!("../templates/dashboard/spec/identity.yaml"),
    },
    TemplateFile {
        path: "spec/project.yaml",
        content: include_str!("../templates/dashboard/spec/project.yaml"),
    },
];

const SAAS_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "app/backend/Cargo.toml",
        content: include_str!("../templates/common/app-backend.Cargo.toml"),
    },
    TemplateFile {
        path: "app/backend/src/lib.rs",
        content: include_str!("../templates/common/app-backend.lib.rs"),
    },
    TemplateFile {
        path: ".gitignore",
        content: include_str!("../templates/common/gitignore"),
    },
    TemplateFile {
        path: ".env.example",
        content: include_str!("../templates/saas/env.example"),
    },
    TemplateFile {
        path: "README.md",
        content: include_str!("../templates/saas/README.md"),
    },
    TemplateFile {
        path: "appstruct.yaml",
        content: include_str!("../templates/saas/appstruct.yaml"),
    },
    TemplateFile {
        path: "compose.yaml",
        content: include_str!("../templates/saas/compose.yaml"),
    },
    TemplateFile {
        path: "rust-toolchain.toml",
        content: include_str!("../templates/common/rust-toolchain.toml"),
    },
    TemplateFile {
        path: "spec/identity.yaml",
        content: include_str!("../templates/saas/spec/identity.yaml"),
    },
    TemplateFile {
        path: "spec/work.yaml",
        content: include_str!("../templates/saas/spec/work.yaml"),
    },
];

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_project_names() {
        for name in ["", "../demo", "Demo", "demo_name", "demo-"] {
            assert!(validate_name(name).is_err(), "accepted {name:?}");
        }
        assert!(validate_name("project-42").is_ok());
    }
}
