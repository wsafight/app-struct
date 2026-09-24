use serde::Serialize;
use std::process::ExitCode;

#[derive(Serialize)]
struct CapabilityReport {
    auth: AuthCapabilities,
    billing: BillingCapabilities,
}

#[derive(Serialize)]
struct AuthCapabilities {
    providers: Vec<ProviderCapability>,
}

#[derive(Serialize)]
struct BillingCapabilities {
    providers: Vec<ProviderCapability>,
}

#[derive(Serialize)]
struct ProviderCapability {
    id: &'static str,
    status: &'static str,
    capabilities: &'static [&'static str],
    required_env: &'static [&'static str],
}

const OIDC_CAPABILITIES: &[&str] = &["login", "signup", "account_linking"];
const BILLING_CAPABILITIES: &[&str] = &[
    "subscriptions",
    "trials",
    "customer_portal",
    "stripe_webhooks",
];
const OIDC_ENV: &[&str] = &[
    "APPSTRUCT_OIDC_AUTHORIZATION_URL",
    "APPSTRUCT_OIDC_TOKEN_URL",
    "APPSTRUCT_OIDC_USERINFO_URL",
    "APPSTRUCT_OIDC_CLIENT_ID",
    "APPSTRUCT_OIDC_CLIENT_SECRET",
    "APPSTRUCT_OIDC_REDIRECT_URI",
];
const GOOGLE_ENV: &[&str] = &[
    "APPSTRUCT_GOOGLE_CLIENT_ID",
    "APPSTRUCT_GOOGLE_CLIENT_SECRET",
    "APPSTRUCT_GOOGLE_REDIRECT_URI",
];
const GITHUB_ENV: &[&str] = &[
    "APPSTRUCT_GITHUB_CLIENT_ID",
    "APPSTRUCT_GITHUB_CLIENT_SECRET",
    "APPSTRUCT_GITHUB_REDIRECT_URI",
];
const STRIPE_ENV: &[&str] = &[
    "APPSTRUCT_STRIPE_SECRET_KEY",
    "APPSTRUCT_STRIPE_WEBHOOK_SECRET",
    "APPSTRUCT_STRIPE_PRICE_<PLAN>",
];

pub(crate) fn run() -> ExitCode {
    let report = CapabilityReport {
        auth: AuthCapabilities {
            providers: vec![
                ProviderCapability {
                    id: "oidc",
                    status: "supported",
                    capabilities: OIDC_CAPABILITIES,
                    required_env: OIDC_ENV,
                },
                ProviderCapability {
                    id: "google",
                    status: "supported",
                    capabilities: OIDC_CAPABILITIES,
                    required_env: GOOGLE_ENV,
                },
                ProviderCapability {
                    id: "github",
                    status: "supported",
                    capabilities: OIDC_CAPABILITIES,
                    required_env: GITHUB_ENV,
                },
            ],
        },
        billing: BillingCapabilities {
            providers: vec![ProviderCapability {
                id: "stripe",
                status: "supported",
                capabilities: BILLING_CAPABILITIES,
                required_env: STRIPE_ENV,
            }],
        },
    };
    if crate::report::is_json() {
        crate::report::success(&report);
    } else {
        println!("Auth providers:");
        for provider in &report.auth.providers {
            println!(
                "- {}: {} ({})",
                provider.id,
                provider.status,
                provider.capabilities.join(", ")
            );
        }
        println!("Billing providers:");
        for provider in &report.billing.providers {
            println!("- {}: {}", provider.id, provider.status);
        }
    }
    ExitCode::SUCCESS
}
