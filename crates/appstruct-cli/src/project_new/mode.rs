use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum DatabaseMode {
    External,
    Managed,
}

impl DatabaseMode {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::External => "external",
            Self::Managed => "managed",
        }
    }
}
