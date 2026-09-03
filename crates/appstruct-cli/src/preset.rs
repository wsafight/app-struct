use clap::Subcommand;
use std::{path::Path, process::ExitCode};

#[derive(Debug, Subcommand)]
pub(crate) enum PresetCommand {
    /// Show the selected preset, or its canonical expanded module configuration.
    Show {
        /// Print the complete module defaults after preset expansion.
        #[arg(long)]
        expanded: bool,
    },
}

pub(crate) fn run(project: &Path, command: &PresetCommand) -> ExitCode {
    let ir = match appstruct_compiler::compile_project(project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    let Some(selected) = ir.preset else {
        return crate::report::fail(
            "AS6007",
            crate::report::ErrorCategory::Configuration,
            "this project does not select a preset",
            crate::report::ExitClass::Validation,
        );
    };
    let Some(info) = appstruct_compiler::preset_info(&selected.name, u64::from(selected.version))
    else {
        return crate::report::fail(
            "AS6007",
            crate::report::ErrorCategory::Configuration,
            "selected preset is not available in this AppStruct version",
            crate::report::ExitClass::Validation,
        );
    };
    match command {
        PresetCommand::Show { expanded: true } => {
            match appstruct_compiler::expanded_preset(project) {
                Ok(Some(expanded)) => {
                    if crate::report::is_json() {
                        crate::report::success(&serde_json::json!({
                            "command": "preset",
                            "action": "show",
                            "expanded": true,
                            "name": info.name,
                            "version": info.version,
                            "digest": info.digest,
                            "modules": info.modules,
                            "source": expanded,
                        }));
                    } else {
                        print!("{expanded}");
                    }
                }
                Ok(None) => unreachable!("compiled IR selected a preset"),
                Err(diagnostics) => {
                    return crate::report::fail_diagnostics(
                        crate::report::ErrorCategory::Validation,
                        diagnostics,
                    );
                }
            }
        }
        PresetCommand::Show { expanded: false } => {
            if crate::report::is_json() {
                crate::report::success(&serde_json::json!({
                    "command": "preset",
                    "action": "show",
                    "expanded": false,
                    "name": info.name,
                    "version": info.version,
                    "digest": info.digest,
                    "modules": info.modules,
                }));
            } else {
                println!("{} {}", info.name, info.version);
                println!("digest: {}", info.digest);
                println!("modules: {}", info.modules.join(", "));
            }
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn show_rejects_projects_without_a_preset_and_missing_projects() {
        assert_ne!(
            run(
                Path::new("/missing-appstruct-project"),
                &PresetCommand::Show { expanded: false },
            ),
            ExitCode::SUCCESS
        );
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
        assert_ne!(
            run(&fixture, &PresetCommand::Show { expanded: false }),
            ExitCode::SUCCESS
        );
        let saas =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m6-preset-project");
        if saas.exists() {
            crate::report::set_output_format(crate::report::OutputFormat::Text);
            let _ = run(&saas, &PresetCommand::Show { expanded: false });
            crate::report::set_output_format(crate::report::OutputFormat::Json);
            let _ = run(&saas, &PresetCommand::Show { expanded: true });
            crate::report::set_output_format(crate::report::OutputFormat::Text);
        }
    }
}
