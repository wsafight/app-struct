use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrValidationError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for IrValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrValidationErrors(pub(super) Vec<IrValidationError>);

impl IrValidationErrors {
    #[must_use]
    pub fn errors(&self) -> &[IrValidationError] {
        &self.0
    }
}

impl fmt::Display for IrValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid AppStruct IR: {}",
            self.0
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

impl Error for IrValidationErrors {}
