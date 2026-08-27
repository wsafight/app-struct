//! Central compatibility policy for persisted and generated `AppStruct` contracts.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionRange {
    pub minimum: u32,
    pub current: u32,
}

impl VersionRange {
    #[must_use]
    pub const fn exact(current: u32) -> Self {
        Self {
            minimum: current,
            current,
        }
    }

    #[must_use]
    pub const fn supports(self, version: u32) -> bool {
        version >= self.minimum && version <= self.current
    }

    #[must_use]
    pub const fn classify(self, version: u32) -> Compatibility {
        if version == self.current {
            Compatibility::Current
        } else if self.supports(version) {
            Compatibility::Compatible
        } else {
            Compatibility::Unsupported
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compatibility {
    Current,
    Compatible,
    Unsupported,
}

pub const IR: VersionRange = VersionRange {
    minimum: 7,
    current: 10,
};
pub const RUNTIME_API: VersionRange = VersionRange::exact(2);
pub const MODULE_API: VersionRange = VersionRange::exact(1);
pub const PROJECT_LAYOUT: VersionRange = VersionRange {
    minimum: 1,
    current: 2,
};
pub const DATABASE_SCHEMA: VersionRange = VersionRange {
    minimum: 1,
    current: 2,
};
pub const OWNERSHIP_MANIFEST: VersionRange = VersionRange::exact(1);
pub const CACHE_SCHEMA: VersionRange = VersionRange::exact(2);
pub const TRANSACTION_JOURNAL: VersionRange = VersionRange::exact(1);

pub const CONTRACT_MATRIX: &[(&str, VersionRange)] = &[
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
    use super::{CONTRACT_MATRIX, Compatibility};

    #[test]
    fn every_contract_classifies_current_legacy_and_future_versions() {
        for (name, range) in CONTRACT_MATRIX {
            assert_eq!(
                range.classify(range.current),
                Compatibility::Current,
                "{name} current"
            );
            assert_eq!(
                range.classify(range.current.saturating_add(1)),
                Compatibility::Unsupported,
                "{name} future"
            );
            if range.minimum < range.current {
                assert_eq!(
                    range.classify(range.minimum),
                    Compatibility::Compatible,
                    "{name} legacy"
                );
            }
            if range.minimum > 0 {
                assert_eq!(
                    range.classify(range.minimum - 1),
                    Compatibility::Unsupported,
                    "{name} too old"
                );
            }
        }
    }
}
