use appstruct_ir::{AppIr, EntityIr};

pub(crate) fn entity_api_name(ir: &AppIr, entity: &EntityIr) -> String {
    let base = lower_camel(&entity.rust_name);
    let mut candidate = if is_reserved_api_name(ir, &base) {
        format!("{base}Resource")
    } else {
        base
    };
    while ir
        .entities
        .iter()
        .any(|other| other.id != entity.id && lower_camel(&other.rust_name) == candidate)
    {
        candidate.push_str("Resource");
    }
    format!("{candidate}Api")
}

fn is_reserved_api_name(ir: &AppIr, name: &str) -> bool {
    name == "savedViews"
        || (ir.auth.enabled && matches!(name, "auth" | "admin" | "product" | "business"))
        || (ir.billing.enabled && matches!(name, "billing" | "billingAdmin"))
        || (ir.file.enabled && ir.auth.enabled && name == "file")
        || (ir.tenant.enabled && name == "tenant")
        || (ir.audit.enabled && name == "audit")
        || (ir.report.enabled && name == "report")
        || (ir.activity.enabled && name == "activity")
}

fn lower_camel(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}
