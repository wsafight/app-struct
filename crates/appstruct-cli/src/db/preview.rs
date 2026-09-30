use super::render::Draft;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub(super) struct PullPreview {
    pub(super) entities: Vec<PullEntityPreview>,
    pub(super) migration: &'static str,
}

#[derive(Clone, Serialize)]
pub(super) struct PullEntityPreview {
    pub(super) entity: String,
    pub(super) page: String,
    pub(super) api: String,
}

pub(super) fn build(draft: &Draft) -> PullPreview {
    PullPreview {
        entities: draft
            .entity_names
            .iter()
            .map(|entity| {
                let slug = entity.to_ascii_lowercase();
                PullEntityPreview {
                    entity: entity.clone(),
                    page: format!("/admin/{slug}"),
                    api: format!("/api/{slug}"),
                }
            })
            .collect(),
        migration: "new imported entities will be included in the next migration",
    }
}

pub(super) fn print(draft: &Draft) {
    println!("Preview:");
    for entity in &draft.entity_names {
        let slug = entity.to_ascii_lowercase();
        println!("  {entity}: page /admin/{slug}, API /api/{slug}");
    }
    println!("  Migration: new imported entities will be included in the next migration");
}
