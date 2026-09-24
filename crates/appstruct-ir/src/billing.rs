use serde::{Deserialize, Serialize};

/// Billing facts known at compile time. Provider secrets are deliberately not part of the IR.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingIr {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subscriptions: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trials: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub customer_portal: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub metered_usage: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plans: Vec<BillingPlanIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingPlanIr {
    pub id: String,
    pub price_env: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_days: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entitlements: Vec<String>,
}
