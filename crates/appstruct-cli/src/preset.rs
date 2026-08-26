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
            for diagnostic in &diagnostics {
                crate::render_text_diagnostic(diagnostic);
            }
            return ExitCode::from(1);
        }
    };
    let Some(selected) = ir.preset else {
        eprintln!("error[AS6007]: this project does not select a preset");
        return ExitCode::from(1);
    };
    let Some(info) = appstruct_compiler::preset_info(&selected.name, u64::from(selected.version))
    else {
        eprintln!("error[AS6007]: selected preset is not available in this AppStruct version");
        return ExitCode::from(1);
    };
    match command {
        PresetCommand::Show { expanded: true } => print!("{}", info.expanded),
        PresetCommand::Show { expanded: false } => {
            println!("{} {}", info.name, info.version);
            println!("digest: {}", info.digest);
            println!("modules: {}", info.modules.join(", "));
        }
    }
    ExitCode::SUCCESS
}
