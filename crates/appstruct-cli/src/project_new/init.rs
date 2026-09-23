use super::{DatabaseMode, ProjectTemplate, name, run_with_command};
use std::fmt::Write as _;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

#[derive(Clone, Copy)]
pub(super) struct InitSettings {
    pub(super) database_mode: DatabaseMode,
    pub(super) api_port: u16,
    pub(super) web_port: u16,
}

impl InitSettings {
    pub(super) fn apply(
        self,
        root: &Path,
        name: &str,
        template: ProjectTemplate,
    ) -> io::Result<()> {
        match (template, self.database_mode) {
            (ProjectTemplate::Minimal, DatabaseMode::Managed) => {
                fs::write(
                    root.join("compose.yaml"),
                    include_str!("../../templates/dashboard/compose.yaml"),
                )?;
            }
            (ProjectTemplate::Dashboard | ProjectTemplate::Saas, DatabaseMode::External) => {
                fs::remove_file(root.join("compose.yaml"))?;
            }
            _ => {}
        }
        let database_url = match self.database_mode {
            DatabaseMode::Managed => {
                "postgresql://appstruct:appstruct-dev@127.0.0.1:5432/appstruct?sslmode=disable"
                    .to_owned()
            }
            DatabaseMode::External => {
                format!("postgresql://postgres@127.0.0.1:5432/{name}?sslmode=disable")
            }
        };
        let mut example = format!("DATABASE_URL={database_url}\n");
        if !matches!(template, ProjectTemplate::Minimal) {
            let _ = writeln!(
                example,
                "APPSTRUCT_ALLOWED_ORIGIN=http://127.0.0.1:{}",
                self.web_port
            );
            match template {
                ProjectTemplate::Dashboard => example.push_str("APPSTRUCT_AUTH_MAIL_MODE=dev\n"),
                ProjectTemplate::Saas => example.push_str(
                    "APPSTRUCT_AUTH_MAIL_MODE=capture\nAPPSTRUCT_ENV=development\nAPPSTRUCT_FILE_ROOT=.appstruct/files\n",
                ),
                ProjectTemplate::Minimal => unreachable!(),
            }
        }
        fs::write(root.join(".env.example"), example)?;
        fs::write(
            root.join(".env"),
            format!(
                "APPSTRUCT_API_PORT={}\nAPPSTRUCT_WEB_PORT={}\n",
                self.api_port, self.web_port
            ),
        )
    }
}

pub(crate) fn run(
    parent: &Path,
    name: Option<&str>,
    template: Option<ProjectTemplate>,
    database_mode: Option<DatabaseMode>,
    api_port: Option<u16>,
    web_port: Option<u16>,
) -> ExitCode {
    let needs_prompt = name.is_none() || template.is_none();
    if needs_prompt && (crate::report::is_json() || !io::stdin().is_terminal()) {
        return crate::report::fail(
            "AS6003",
            crate::report::ErrorCategory::Project,
            "appstruct init needs a terminal and text output when name or template is omitted; pass a project name and --template for non-interactive use",
            crate::report::ExitClass::Usage,
        );
    }

    let name = match name {
        Some(name) => name.to_owned(),
        None => match prompt_name() {
            Ok(name) => name,
            Err(error) => {
                return crate::report::fail(
                    "AS6004",
                    crate::report::ErrorCategory::Project,
                    format!("cannot read project name: {error}"),
                    crate::report::ExitClass::Usage,
                );
            }
        },
    };
    let template = match template {
        Some(template) => template,
        None => match prompt_template() {
            Ok(template) => template,
            Err(error) => {
                return crate::report::fail(
                    "AS6005",
                    crate::report::ErrorCategory::Project,
                    format!("cannot read project template: {error}"),
                    crate::report::ExitClass::Usage,
                );
            }
        },
    };

    if api_port == Some(0) || web_port == Some(0) || api_port.is_some() && api_port == web_port {
        return invalid_ports();
    }
    let database_mode = match database_mode {
        Some(mode) => mode,
        None if needs_prompt => match prompt_database_mode(template.database_mode()) {
            Ok(mode) => mode,
            Err(error) => return prompt_error("database mode", &error),
        },
        None => template.database_mode(),
    };
    let api_port = match api_port {
        Some(port) => port,
        None if needs_prompt => match prompt_port("API port [3000]: ", 3000, None) {
            Ok(port) => port,
            Err(error) => return prompt_error("API port", &error),
        },
        None => 3000,
    };
    let web_port = match web_port {
        Some(port) => port,
        None if needs_prompt => match prompt_port("Web port [5173]: ", 5173, Some(api_port)) {
            Ok(port) => port,
            Err(error) => return prompt_error("web port", &error),
        },
        None => 5173,
    };
    if api_port == web_port {
        return invalid_ports();
    }
    let settings = InitSettings {
        database_mode,
        api_port,
        web_port,
    };
    run_with_command(parent, &name, template, "init", Some(settings))
}

fn invalid_ports() -> ExitCode {
    crate::report::fail(
        "AS6006",
        crate::report::ErrorCategory::Project,
        "API and web ports must be non-zero and different",
        crate::report::ExitClass::Usage,
    )
}

fn prompt_error(field: &str, error: &io::Error) -> ExitCode {
    crate::report::fail(
        "AS6007",
        crate::report::ErrorCategory::Project,
        format!("cannot read {field}: {error}"),
        crate::report::ExitClass::Usage,
    )
}

fn prompt_line(prompt: &str, default: Option<&str>) -> io::Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "input ended before a choice was entered",
        ));
    }
    let value = input.lines().next().unwrap_or_default().trim();
    if value.is_empty() {
        Ok(default.unwrap_or_default().to_owned())
    } else {
        Ok(value.to_owned())
    }
}

fn prompt_name() -> io::Result<String> {
    loop {
        let name = prompt_line("Project name: ", None)?;
        match name::validate_name(&name) {
            Ok(()) => return Ok(name),
            Err(error) => eprintln!("{error}"),
        }
    }
}

fn prompt_template() -> io::Result<ProjectTemplate> {
    println!("Choose a template:");
    println!("  1) minimal   Public Note app with external PostgreSQL");
    println!("  2) dashboard Auth, RBAC, owner scope, and managed PostgreSQL");
    println!("  3) saas      Tenant, Audit, Mail, Jobs, and File modules");
    loop {
        let choice = prompt_line("Template [2]: ", Some("2"))?;
        if let Some(template) = ProjectTemplate::from_choice(&choice) {
            return Ok(template);
        }
        eprintln!("template must be 1 (minimal), 2 (dashboard), or 3 (saas)");
    }
}

fn prompt_database_mode(default: DatabaseMode) -> io::Result<DatabaseMode> {
    println!("Database: 1) external PostgreSQL  2) managed PostgreSQL (Docker)");
    let label = match default {
        DatabaseMode::External => "1",
        DatabaseMode::Managed => "2",
    };
    loop {
        let choice = prompt_line(&format!("Database mode [{label}]: "), Some(label))?;
        match choice.as_str() {
            "1" | "external" => return Ok(DatabaseMode::External),
            "2" | "managed" => return Ok(DatabaseMode::Managed),
            _ => eprintln!("database mode must be 1 (external) or 2 (managed)"),
        }
    }
}

fn prompt_port(label: &str, default: u16, other: Option<u16>) -> io::Result<u16> {
    let fallback = default.to_string();
    loop {
        let input = prompt_line(label, Some(&fallback))?;
        match input.parse::<u16>() {
            Ok(port) if port != 0 && Some(port) != other => return Ok(port),
            _ => eprintln!("port must be 1-65535 and different from the other port"),
        }
    }
}

impl ProjectTemplate {
    fn from_choice(choice: &str) -> Option<Self> {
        match choice {
            "1" | "minimal" => Some(Self::Minimal),
            "2" | "dashboard" => Some(Self::Dashboard),
            "3" | "saas" => Some(Self::Saas),
            _ => None,
        }
    }
}
