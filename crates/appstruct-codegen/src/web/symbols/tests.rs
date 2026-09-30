use super::{ProvidedSymbols, RegistryScan, RequiredSymbols, scan_registry};
use std::path::Path;

fn registry(source: &str) -> Option<ProvidedSymbols> {
    match scan_registry(Path::new("registry.tsx"), source) {
        RegistryScan::Conclusive { provided, .. } => Some(provided),
        RegistryScan::Inconclusive => None,
    }
}

#[test]
fn registry_members_remain_in_their_own_namespaces() {
    let provided = registry(
        "export const registry = { fields: { Shared }, pages: { Dashboard } } satisfies AppStructRegistry;",
    )
    .unwrap();
    assert_eq!(provided.field_components, ["Shared".to_owned()].into());
    assert_eq!(provided.page_components, ["Dashboard".to_owned()].into());
}

#[test]
fn define_registry_and_nested_type_assertions_are_recognized() {
    let provided = registry(
        "export const registry = defineAppStructRegistry(({ fields: { Editor }, pages: {} } as const) satisfies AppStructRegistry);",
    )
    .unwrap();
    assert!(provided.field_components.contains("Editor"));
}

#[test]
fn unrelated_exports_do_not_satisfy_registry_keys() {
    let provided = registry(
        "export function Dashboard() {}\nexport const registry = { fields: {}, pages: {} };",
    )
    .unwrap();
    assert!(!provided.page_components.contains("Dashboard"));
}

#[test]
fn local_named_registry_export_is_recognized() {
    let provided =
        registry("const registry = { fields: { Editor }, pages: {} };\nexport { registry };")
            .unwrap();
    assert!(provided.field_components.contains("Editor"));
}

#[test]
fn dynamic_shapes_parse_errors_and_reexports_are_inconclusive() {
    for source in [
        "export const registry = { fields: { ...fields }, pages: {} };",
        "export const { = broken",
        "export * from './components';",
    ] {
        assert!(registry(source).is_none(), "{source}");
    }
}

#[test]
fn unrelated_named_reexports_do_not_disable_the_check() {
    let provided = registry(
        "export { helper } from './helper';\nexport const registry = { fields: {}, pages: {} };",
    )
    .unwrap();
    assert!(provided.field_components.is_empty());
    assert!(provided.page_components.is_empty());
}

#[test]
fn required_symbols_report_is_empty_without_custom_components() {
    assert!(RequiredSymbols::default().is_empty());
}
