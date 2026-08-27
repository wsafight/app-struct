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
                Ok(Some(expanded)) => print!("{expanded}"),
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
            println!("{} {}", info.name, info.version);
            println!("digest: {}", info.digest);
            println!("modules: {}", info.modules.join(", "));
        }
    }
    ExitCode::SUCCESS
}
