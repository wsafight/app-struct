use super::{ProjectTemplate, name, run_with_command};
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

pub(crate) fn run(
    parent: &Path,
    name: Option<&str>,
    template: Option<ProjectTemplate>,
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

    run_with_command(parent, &name, template, "init")
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
