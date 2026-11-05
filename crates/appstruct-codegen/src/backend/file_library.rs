use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::{AppIr, FieldTypeIr};

pub(super) fn artifact(ir: &AppIr) -> Result<Artifact, CodegenError> {
    let arms = ir.entities.iter().map(|entity| {
        let module = super::module_name(entity);
        format!("{name:?} => if write {{ crate::api::{module}::authorize_attachment_write(state, context, record_id).await }} else {{ crate::api::{module}::authorize_activity_target(state, context, record_id).await }}", name = entity.table_name)
    }).collect::<Vec<_>>().join(",\n");
    let existence = ir.entities.iter().map(|entity| {
        let primary = entity.fields.iter().find(|field| field.primary_key).expect("validated primary key");
        let quote = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
        let scope = if entity.tenant_scoped { " AND tenant_id = $2" } else { " AND $2::uuid IS NOT DISTINCT FROM $2::uuid" };
        let query = format!("SELECT 1 FROM {} WHERE {}::text = $1{scope}", quote(&entity.table_name), quote(&primary.column_name));
        format!("{:?} => database.fetch_optional(Sql::bind({query:?}, [record_id.into(), tenant.into()])).await?.is_some()", entity.table_name)
    }).collect::<Vec<_>>().join(",\n");
    let canonical = ir.entities.iter().map(|entity| {
        let primary = entity.fields.iter().find(|field| field.primary_key).expect("validated primary key");
        let expression = match primary.ty {
            FieldTypeIr::Uuid => "record_id.parse::<uuid::Uuid>().map(|value| value.to_string()).map_err(|_| ApiError::NotFound)",
            FieldTypeIr::Integer => "record_id.parse::<i32>().map(|value| value.to_string()).map_err(|_| ApiError::NotFound)",
            FieldTypeIr::Bigint => "record_id.parse::<i64>().map(|value| value.to_string()).map_err(|_| ApiError::NotFound)",
            _ => "Ok(record_id.to_owned())",
        };
        format!("{:?} => {expression}", entity.table_name)
    }).collect::<Vec<_>>().join(",\n");
    let source = if ir.file.enabled && ir.auth.enabled {
        include_str!("../../templates/backend/file_library.rs")
            .replace("__RESOURCE_ARMS__", &arms)
            .replace("__EXISTENCE_ARMS__", &existence)
            .replace("__CANONICAL_ARMS__", &canonical)
            .replace("__MAX_BYTES__", &ir.file.max_bytes.to_string())
            .replace(
                "__SCOPE__",
                if ir.tenant.enabled {
                    "context.require_tenant()?.to_string()"
                } else {
                    "context.actor().ok_or(ApiError::Unauthorized)?.id.to_string()"
                },
            )
    } else {
        "use crate::AppState;\nuse axum::Router;\npub fn router() -> Router<AppState> { Router::new() }".to_owned()
    };
    Ok(Artifact::text(
        "backend/src/file_library.rs",
        super::rust_template(&source)?,
        ArtifactKind::RustSource,
    ))
}
