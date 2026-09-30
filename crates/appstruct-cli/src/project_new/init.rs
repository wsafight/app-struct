use super::{CapabilityMode, DatabaseMode, ProjectTemplate, name, run_with_command};
use clap::Args;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    pub name: Option<String>,
    #[arg(long, value_enum)]
    pub template: Option<ProjectTemplate>,
    #[arg(long, value_enum)]
    pub database_mode: Option<DatabaseMode>,
    #[arg(long)]
    pub api_port: Option<u16>,
    #[arg(long)]
    pub web_port: Option<u16>,
    #[arg(long, value_enum)]
    pub auth: Option<CapabilityMode>,
    #[arg(long, value_enum)]
    pub tenant: Option<CapabilityMode>,
}

#[derive(Clone, Copy)]
pub(super) struct InitSettings {
    pub(super) database_mode: DatabaseMode,
    pub(super) api_port: u16,
    pub(super) web_port: u16,
    pub(super) auth: CapabilityMode,
    pub(super) tenant: CapabilityMode,
}

impl InitSettings {
    pub(super) fn apply(
        self,
        root: &Path,
        name: &str,
        template: ProjectTemplate,
    ) -> io::Result<()> {
        let appstruct_path = root.join("appstruct.yaml");
        let source = fs::read_to_string(&appstruct_path)?;
        let source = render_capabilities(source, template, self.auth, self.tenant)?;
        fs::write(appstruct_path, source)?;
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

fn render_capabilities(
    source: String,
    template: ProjectTemplate,
    auth: CapabilityMode,
    tenant: CapabilityMode,
) -> io::Result<String> {
    let valid = match template {
        ProjectTemplate::Minimal => {
            auth == CapabilityMode::Disabled && tenant == CapabilityMode::Disabled
        }
        ProjectTemplate::Dashboard => auth == CapabilityMode::Enabled,
        ProjectTemplate::Saas => {
            auth == CapabilityMode::Enabled && tenant == CapabilityMode::Enabled
        }
    };
    if !valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "template `{}` does not support auth={} and tenant={}",
                template.name(),
                auth.name(),
                tenant.name()
            ),
        ));
    }
    let mut output = source;
    if template == ProjectTemplate::Dashboard {
        output = output.replace(
            "    enabled: true\n",
            &format!("    enabled: {}\n", auth == CapabilityMode::Enabled),
        );
        if tenant == CapabilityMode::Enabled {
            output = output.replace(
                "    default_role: member\n",
                "    default_role: member\n  tenant:\n    enabled: true\n",
            );
        }
    }
    if template == ProjectTemplate::Saas {
        output.push_str("\nmodules:\n  auth:\n    enabled: true\n  tenant:\n    enabled: true\n");
    }
    Ok(output)
}

pub(crate) fn run(parent: &Path, args: &InitArgs) -> ExitCode {
    let name = args.name.as_deref();
    let template = args.template;
    let database_mode = args.database_mode;
    let api_port = args.api_port;
    let web_port = args.web_port;
    let auth = args.auth;
    let tenant = args.tenant;
    let needs_prompt = name.is_none() || template.is_none();
    let interactive = io::stdin().is_terminal() && !crate::report::is_json();
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
        None if interactive => match prompt_database_mode(template.database_mode()) {
            Ok(mode) => mode,
            Err(error) => return prompt_error("database mode", &error),
        },
        None => template.database_mode(),
    };
    let api_port = match api_port {
        Some(port) => port,
        None if interactive => match prompt_port("API port [3000]: ", 3000, None) {
            Ok(port) => port,
            Err(error) => return prompt_error("API port", &error),
        },
        None => 3000,
    };
    let web_port = match web_port {
        Some(port) => port,
        None if interactive => match prompt_port("Web port [5173]: ", 5173, Some(api_port)) {
            Ok(port) => port,
            Err(error) => return prompt_error("web port", &error),
        },
        None => 5173,
    };
    if api_port == web_port {
        return invalid_ports();
    }
    let auth = match auth {
        Some(value) => value,
        None if interactive => match prompt_capability("Auth", template.auth_mode()) {
            Ok(value) => value,
            Err(error) => return prompt_error("auth capability", &error),
        },
        None => template.auth_mode(),
    };
    let tenant = match tenant {
        Some(value) => value,
        None if interactive => match prompt_capability("Tenant", template.tenant_mode()) {
            Ok(value) => value,
            Err(error) => return prompt_error("tenant capability", &error),
        },
        None => template.tenant_mode(),
    };
    let settings = InitSettings {
        database_mode,
        api_port,
        web_port,
        auth,
        tenant,
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

fn prompt_capability(label: &str, default: CapabilityMode) -> io::Result<CapabilityMode> {
    let fallback = default.name();
    loop {
        let input = prompt_line(
            &format!("{label} capability [{fallback}]: "),
            Some(fallback),
        )?;
        match input.as_str() {
            "enabled" | "on" | "true" => return Ok(CapabilityMode::Enabled),
            "disabled" | "off" | "false" => return Ok(CapabilityMode::Disabled),
            _ => eprintln!("{label} must be enabled or disabled"),
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

    fn auth_mode(self) -> CapabilityMode {
        match self {
            Self::Minimal => CapabilityMode::Disabled,
            Self::Dashboard | Self::Saas => CapabilityMode::Enabled,
        }
    }

    fn tenant_mode(self) -> CapabilityMode {
        match self {
            Self::Saas => CapabilityMode::Enabled,
            Self::Minimal | Self::Dashboard => CapabilityMode::Disabled,
        }
    }
}
