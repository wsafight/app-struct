//! Current format versions for persisted and generated `AppStruct` contracts.

/// Source files embedded by the generated backend runtime.
#[doc(hidden)]
pub mod __source {
    pub const LIB: &str = include_str!("lib.rs");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractVersion {
    pub current: u32,
}

impl ContractVersion {
    #[must_use]
    pub const fn exact(current: u32) -> Self {
        Self { current }
    }

    #[must_use]
    pub const fn supports(self, version: u32) -> bool {
        version == self.current
    }
}

pub const IR: ContractVersion = ContractVersion::exact(18);
pub const RUNTIME_API: ContractVersion = ContractVersion::exact(5);
pub const MODULE_API: ContractVersion = ContractVersion::exact(1);
pub const PROJECT_LAYOUT: ContractVersion = ContractVersion::exact(2);
pub const DATABASE_SCHEMA: ContractVersion = ContractVersion::exact(3);
pub const OWNERSHIP_MANIFEST: ContractVersion = ContractVersion::exact(1);
pub const CACHE_SCHEMA: ContractVersion = ContractVersion::exact(2);
pub const TRANSACTION_JOURNAL: ContractVersion = ContractVersion::exact(1);

pub const CONTRACT_MATRIX: &[(&str, ContractVersion)] = &[
    ("ir", IR),
    ("runtime_api", RUNTIME_API),
    ("module_api", MODULE_API),
    ("project_layout", PROJECT_LAYOUT),
    ("database_schema", DATABASE_SCHEMA),
    ("ownership_manifest", OWNERSHIP_MANIFEST),
    ("cache_schema", CACHE_SCHEMA),
    ("transaction_journal", TRANSACTION_JOURNAL),
];

#[cfg(test)]
mod tests {
    #[test]
    fn contracts_accept_only_the_current_format() {
        for (name, version) in super::CONTRACT_MATRIX {
            assert!(version.supports(version.current), "{name}");
            assert!(!version.supports(version.current + 1), "{name}");
            assert!(!version.supports(version.current - 1), "{name}");
        }
        assert_eq!(super::__source::LIB.len(), include_str!("lib.rs").len());
    }
}
