use appstruct_compiler::APP_SPEC_SCHEMA;
use serde_json::{Value, json};

#[test]
fn app_spec_schema_accepts_root_and_domain_contracts() {
    let schema: Value = serde_json::from_str(APP_SPEC_SCHEMA).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();

    let root = json!({
        "version": 1,
        "app": { "name": "project-hub" },
        "database": {
            "provider": "postgres",
            "dev": { "mode": "external", "migration": "unmanaged" }
        },
        "preset": { "name": "appstruct/saas", "version": 1 },
        "modules": {
            "mail": {
                "provider": "smtp",
                "templates": {
                    "project-created": { "subject": "Welcome", "text": "Hello" }
                }
            },
            "jobs": {
                "enabled": true,
                "queues": { "mail": { "max_attempts": 8, "backoff_seconds": 5 } }
            }
        },
        "module_manifests": ["modules/example/module.toml"],
        "includes": ["spec/project.yaml"]
    });
    assert!(validator.is_valid(&root));
    for policy in ["auto", "prompt", "never", "unmanaged"] {
        let mut candidate = root.clone();
        candidate["database"]["dev"]["migration"] = policy.into();
        assert!(validator.is_valid(&candidate), "{policy}");
    }

    let domain = json!({
        "domain": "project",
        "entities": {
            "Project": {
                "fields": {
                    "id": { "type": "uuid", "primary_key": true, "generated": "uuid_v7" },
                    "owner": { "type": "relation", "target": "User", "required": true },
                    "metadata": {
                        "type": "json",
                        "ui": { "component": "MetadataEditor" },
                        "access": {
                            "read": { "role": "admin" },
                            "write": { "authenticated": true }
                        }
                    },
                    "amount": {
                        "type": "decimal",
                        "ui": {
                            "semantic": "money",
                            "currency_field": "currency",
                            "fraction_digits": 2
                        }
                    },
                    "currency": { "type": "enum", "values": ["CNY", "USD"] }
                },
                "seeds": {
                    "demo": { "id": "00000000-0000-0000-0000-000000000001", "email": "demo@example.com" }
                },
                "indexes": [{ "fields": ["email"], "unique": true, "where": "email IS NOT NULL" }],
                "access": {
                    "read": { "any": [{ "owner": "owner" }, { "role": "admin" }] }
                },
                "tenant": true,
                "audit": true
            }
        },
        "value_objects": {
            "ArchiveInput": { "fields": { "reason": { "type": "string" } } }
        },
        "commands": {
            "ArchiveProject": {
                "input": "ArchiveInput",
                "output": "Project",
                "access": { "authenticated": true }
            }
        },
        "queries": {
            "ProjectSummary": { "output": "Project", "access": { "public": true } }
        },
        "pages": {
            "ProjectDashboard": {
                "path": "project-dashboard",
                "component": "ProjectDashboard"
            }
        }
    });
    assert!(validator.is_valid(&domain));
}

#[test]
fn app_spec_schema_rejects_unknown_keys_and_invalid_access() {
    let schema: Value = serde_json::from_str(APP_SPEC_SCHEMA).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let unknown_root = json!({
        "version": 1,
        "app": { "name": "demo" },
        "database": { "provider": "postgres" },
        "includes": [],
        "unknown": true
    });
    assert!(!validator.is_valid(&unknown_root));

    let invalid_access = json!({
        "domain": "project",
        "entities": {
            "Project": {
                "fields": { "id": { "type": "uuid" } },
                "access": { "read": { "public": true, "role": "admin" } }
            }
        }
    });
    assert!(!validator.is_valid(&invalid_access));

    let invalid_migration = json!({
        "version": 1,
        "app": { "name": "demo" },
        "database": { "provider": "postgres", "dev": { "migration": "sometimes" } },
        "includes": []
    });
    assert!(!validator.is_valid(&invalid_migration));

    let invalid_semantic = json!({
        "domain": "project",
        "entities": {
            "Project": {
                "fields": {
                    "amount": {
                        "type": "decimal",
                        "ui": { "semantic": "money", "component": "MoneyInput" }
                    }
                }
            }
        }
    });
    assert!(!validator.is_valid(&invalid_semantic));
}
