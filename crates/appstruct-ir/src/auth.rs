use crate::EntityId;
use serde::{Deserialize, Serialize};

/// Authentication facts known at compile time in the M0 compiler.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthIr {
    pub enabled: bool,
    pub user_entity: Option<EntityId>,
    pub registration_enabled: bool,
    pub password_reset_enabled: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub oauth_enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oauth_providers: Vec<String>,
    pub roles: Vec<String>,
    pub default_role: Option<String>,
}
