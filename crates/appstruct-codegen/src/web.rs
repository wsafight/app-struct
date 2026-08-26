use crate::{Artifact, ArtifactKind, generated_header};
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr};
use std::fmt::Write;

pub(crate) fn plan(ir: &AppIr) -> Vec<Artifact> {
    let static_files = [
        (
            "web/index.html",
            include_str!("../templates/web/index.html"),
        ),
        (
            "web/src/main.tsx",
            include_str!("../templates/web/main.tsx"),
        ),
        (
            "web/src/resource.ts",
            include_str!("../templates/web/resource.ts"),
        ),
        (
            "web/src/app/App.tsx",
            include_str!("../templates/web/App.tsx"),
        ),
        (
            "web/src/app/Layout.tsx",
            include_str!("../templates/web/Layout.tsx"),
        ),
        (
            "web/src/pages/ResourceList.tsx",
            include_str!("../templates/web/ResourceList.tsx"),
        ),
        (
            "web/src/pages/ResourceForm.tsx",
            include_str!("../templates/web/ResourceForm.tsx"),
        ),
        (
            "web/src/styles.css",
            include_str!("../templates/web/styles.css"),
        ),
        (
            "web/package.json",
            include_str!("../templates/web/package.json"),
        ),
        (
            "web/pnpm-lock.yaml",
            include_str!("../templates/web/pnpm-lock.yaml"),
        ),
    ];
    let mut artifacts = static_files
        .into_iter()
        .map(|(path, content)| Artifact::text(path, content, ArtifactKind::Web))
        .collect::<Vec<_>>();
    artifacts.extend([
        Artifact::text("web/tsconfig.json", tsconfig(), ArtifactKind::Web),
        Artifact::text("web/vite.config.ts", vite_config(), ArtifactKind::Web),
        Artifact::text(
            "web/src/generated/resources.ts",
            resources_source(ir),
            ArtifactKind::TypeScript,
        ),
    ]);
    artifacts
}

fn tsconfig() -> &'static str {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "types": ["vite/client"]
  },
  "include": ["src", "vite.config.ts"]
}
"#
}

fn vite_config() -> &'static str {
    r#"import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: { host: "127.0.0.1", port: 5173 },
});
"#
}

fn resources_source(ir: &AppIr) -> String {
    let imports = ir
        .entities
        .iter()
        .map(|entity| {
            format!(
                "import {{ {}Api }} from \"./client\";",
                lower_camel(&entity.rust_name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let resources = ir
        .entities
        .iter()
        .map(resource_source)
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{}import type {{ ResourceApi, ResourceDefinition }} from \"../resource\";\n{}\n\nexport const resources: ResourceDefinition[] = [\n{}\n];\n",
        generated_header("//"),
        imports,
        indent(&resources, 2)
    )
}

fn resource_source(entity: &EntityIr) -> String {
    let fields = entity
        .fields
        .iter()
        .map(field_source)
        .collect::<Vec<_>>()
        .join(",\n");
    let primary_key = entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .map_or("id", |field| field.rust_name.as_str());
    let api = format!("{}Api", lower_camel(&entity.rust_name));
    format!(
        "{{\n  name: {:?},\n  label: {:?},\n  slug: {:?},\n  primaryKey: {:?},\n  fields: [\n{}\n  ],\n  api: {} as unknown as ResourceApi,\n}}",
        entity.rust_name,
        entity.label,
        entity.table_name,
        primary_key,
        indent(&fields, 4),
        api,
    )
}

fn field_source(field: &FieldIr) -> String {
    let mut properties = vec![
        format!("name: {:?}", field.rust_name),
        format!("label: {:?}", humanize(&field.api_name)),
        format!("kind: {:?}", field_kind(&field.ty)),
        format!("required: {}", !field.nullable),
        format!("readOnly: {}", field.generated.is_some()),
        format!("primaryKey: {}", field.primary_key),
    ];
    if let FieldTypeIr::Enum { values } = &field.ty {
        properties.push(format!("values: {values:?}"));
    }
    if let FieldTypeIr::Relation { target } = &field.ty {
        properties.push(format!("relation: {:?}", target.0));
    }
    format!("{{ {} }}", properties.join(", "))
}

fn field_kind(field_type: &FieldTypeIr) -> &'static str {
    match field_type {
        FieldTypeIr::Uuid => "uuid",
        FieldTypeIr::String => "string",
        FieldTypeIr::Text => "text",
        FieldTypeIr::Integer => "integer",
        FieldTypeIr::Bigint => "bigint",
        FieldTypeIr::Decimal => "decimal",
        FieldTypeIr::Boolean => "boolean",
        FieldTypeIr::Date => "date",
        FieldTypeIr::Datetime => "datetime",
        FieldTypeIr::Json => "json",
        FieldTypeIr::Enum { .. } => "enum",
        FieldTypeIr::Relation { .. } => "relation",
    }
}

fn lower_camel(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}

fn humanize(value: &str) -> String {
    let value = value.strip_suffix("_id").unwrap_or(value);
    let mut words = value.split('_');
    words.next().map_or_else(String::new, |first| {
        let mut characters = first.chars();
        let first = characters
            .next()
            .map_or_else(String::new, |character| character.to_uppercase().collect());
        let mut output = format!("{first}{}", characters.collect::<String>());
        for word in words {
            let _ = write!(output, " {word}");
        }
        output
    })
}

fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
