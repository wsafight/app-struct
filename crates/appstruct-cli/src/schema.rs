use std::io::{self, Write};
use std::process::ExitCode;

pub(crate) fn run() -> ExitCode {
    match io::stdout().write_all(appstruct_compiler::APP_SPEC_SCHEMA.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error[AS6008]: cannot write App Spec schema: {error}");
            ExitCode::from(3)
        }
    }
}
