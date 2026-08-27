use appstruct_ir::{Diagnostic, Severity};
use clap::ValueEnum;
use serde::Serialize;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU8, Ordering};

static OUTPUT_FORMAT: AtomicU8 = AtomicU8::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorCategory {
    Authentication,
    Build,
    Configuration,
    Database,
    Development,
    Generation,
    Migration,
    Project,
    Tooling,
    Transaction,
    Validation,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ExitClass {
    Validation,
    Usage,
    Environment,
    Database,
}

impl ExitClass {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Validation => 1,
            Self::Usage => 2,
            Self::Environment => 3,
            Self::Database => 4,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct CliError {
    pub code: String,
    pub category: ErrorCategory,
    pub message: String,
    pub exit_code: u8,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl CliError {
    pub(crate) fn new(
        code: &str,
        category: ErrorCategory,
        message: impl Into<String>,
        exit: ExitClass,
    ) -> Self {
        Self {
            code: code.to_owned(),
            category,
            message: message.into(),
            exit_code: exit.code(),
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    pub(crate) fn emit(self) -> ExitCode {
        let exit = ExitCode::from(self.exit_code);
        emit_error(&self);
        exit
    }
}

#[derive(Serialize)]
struct ErrorEnvelope<'error> {
    ok: bool,
    error: &'error CliError,
}

#[derive(Serialize)]
struct SuccessEnvelope<'result, T> {
    ok: bool,
    result: &'result T,
}

#[derive(Serialize)]
struct MessageEnvelope<'message> {
    level: &'static str,
    code: &'message str,
    category: ErrorCategory,
    message: &'message str,
}

pub(crate) fn set_output_format(format: OutputFormat) {
    OUTPUT_FORMAT.store(u8::from(format == OutputFormat::Json), Ordering::Release);
}

pub(crate) fn output_format() -> OutputFormat {
    if OUTPUT_FORMAT.load(Ordering::Acquire) == 1 {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    }
}

pub(crate) fn is_json() -> bool {
    output_format() == OutputFormat::Json
}

pub(crate) fn success(result: &impl Serialize) {
    write_json(&SuccessEnvelope { ok: true, result });
}

pub(crate) fn fail(
    code: &str,
    category: ErrorCategory,
    message: impl Into<String>,
    exit: ExitClass,
) -> ExitCode {
    CliError::new(code, category, message, exit).emit()
}

pub(crate) fn fail_diagnostics(category: ErrorCategory, diagnostics: Vec<Diagnostic>) -> ExitCode {
    let message = if diagnostics.len() == 1 {
        diagnostics[0].message.clone()
    } else {
        format!("{} validation diagnostics", diagnostics.len())
    };
    let code = diagnostics
        .first()
        .map_or_else(|| "AS5003".to_owned(), |diagnostic| diagnostic.code.clone());
    CliError::new(&code, category, message, ExitClass::Validation)
        .with_diagnostics(diagnostics)
        .emit()
}

pub(crate) fn warning(code: &str, category: ErrorCategory, message: &str) {
    match output_format() {
        OutputFormat::Text => eprintln!("warning[{code}]: {message}"),
        OutputFormat::Json => write_json_stderr(&MessageEnvelope {
            level: "warning",
            code,
            category,
            message,
        }),
    }
}

fn emit_error(error: &CliError) {
    match output_format() {
        OutputFormat::Text => {
            if error.diagnostics.is_empty() {
                eprintln!("error[{}]: {}", error.code, error.message);
            } else {
                for diagnostic in &error.diagnostics {
                    render_text_diagnostic(diagnostic);
                }
            }
        }
        OutputFormat::Json => write_json(&ErrorEnvelope { ok: false, error }),
    }
}

pub(crate) fn write_json(value: &impl Serialize) {
    match serde_json::to_string_pretty(value) {
        Ok(output) => println!("{output}"),
        Err(error) => eprintln!("error[AS5003]: failed to serialize CLI report: {error}"),
    }
}

fn write_json_stderr(value: &impl Serialize) {
    match serde_json::to_string(value) {
        Ok(output) => eprintln!("{output}"),
        Err(error) => eprintln!("error[AS5003]: failed to serialize CLI report: {error}"),
    }
}

pub(crate) fn render_text_diagnostic(diagnostic: &Diagnostic) {
    let span = &diagnostic.primary.span;
    let severity = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    eprintln!(
        "{}:{}:{}: {severity}[{}]: {}",
        span.file, span.line, span.column, diagnostic.code, diagnostic.message
    );
    if !diagnostic.primary.message.is_empty() {
        eprintln!("  = {}", diagnostic.primary.message);
    }
    for secondary in &diagnostic.secondary {
        eprintln!(
            "  = {}:{}:{}: {}",
            secondary.span.file, secondary.span.line, secondary.span.column, secondary.message
        );
    }
    if let Some(help) = &diagnostic.help {
        eprintln!("  help: {help}");
    }
}
