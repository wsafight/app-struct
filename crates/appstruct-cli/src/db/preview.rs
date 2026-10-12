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
            .entity_tables
            .iter()
            .map(|(entity, table)| PullEntityPreview {
                entity: entity.clone(),
                page: format!("/{table}"),
                api: format!("/api/{table}/"),
            })
            .collect(),
        migration: "new imported entities will be included in the next migration",
    }
}

pub(super) fn print(draft: &Draft) {
    println!("Preview:");
    for entity in build(draft).entities {
        println!(
            "  {}: page {}, API {}",
            entity.entity, entity.page, entity.api
        );
    }
    println!("  Migration: new imported entities will be included in the next migration");
}
