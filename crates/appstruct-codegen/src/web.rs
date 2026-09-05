mod app;
mod resources;

use crate::{Artifact, ArtifactKind, generated_header};
use appstruct_ir::AppIr;

pub(crate) fn plan(ir: &AppIr) -> Vec<Artifact> {
    let mut artifacts = framework_files(ir)
        .into_iter()
        .chain(page_files(ir))
        .map(|(path, content)| Artifact::text(path, content, ArtifactKind::Web))
        .collect::<Vec<_>>();
    extend_module_artifacts(ir, &mut artifacts);
    artifacts.extend([
        Artifact::text("web/tsconfig.json", tsconfig(), ArtifactKind::Web),
        Artifact::text("web/vite.config.ts", vite_config(), ArtifactKind::Web),
        Artifact::text(
            "web/src/generated/resources.ts",
            resources::source(ir),
            ArtifactKind::TypeScript,
        ),
        Artifact::text(
            "web/src/generated/registry.ts",
            registry_source(ir),
            ArtifactKind::TypeScript,
        ),
    ]);
    artifacts
}

fn framework_files(ir: &AppIr) -> Vec<(&'static str, String)> {
    let title = app_display_name(&ir.app.name);
    let main = if app::requires_registry(ir) {
        include_str!("../templates/web/main_with_registry.tsx")
    } else {
        include_str!("../templates/web/main.tsx")
    };
    vec![
        (
            "web/index.html",
            with_app_title(include_str!("../templates/web/index.html"), &title),
        ),
        (
            "web/.gitignore",
            include_str!("../templates/web/gitignore").to_owned(),
        ),
        (
            "web/eslint.config.js",
            include_str!("../templates/web/eslint.config.js").to_owned(),
        ),
        ("web/src/main.tsx", main.to_owned()),
        (
            "web/src/resource.ts",
            include_str!("../templates/web/resource.ts").to_owned(),
        ),
        (
            "web/src/query.ts",
            include_str!("../templates/web/query.ts").to_owned(),
        ),
        (
            "web/src/controller.ts",
            include_str!("../templates/web/controller.ts").to_owned(),
        ),
        (
            "web/src/components/Dialog.tsx",
            include_str!("../templates/web/Dialog.tsx").to_owned(),
        ),
        (
            "web/src/navigation.tsx",
            include_str!("../templates/web/navigation.tsx").to_owned(),
        ),
        (
            "web/src/framework.test.ts",
            include_str!("../templates/web/framework.test.ts").to_owned(),
        ),
        (
            "web/src/interaction.test.tsx",
            include_str!("../templates/web/interaction.test.tsx").to_owned(),
        ),
        ("web/src/app/App.tsx", app::source(ir)),
        (
            "web/src/app/ResourceRoutes.tsx",
            include_str!("../templates/web/ResourceRoutes.tsx").to_owned(),
        ),
        ("web/src/app/Layout.tsx", app::layout_source(ir)),
        (
            "web/src/pages/ResourceList.tsx",
            include_str!("../templates/web/ResourceList.tsx").to_owned(),
        ),
    ]
}

fn page_files(ir: &AppIr) -> Vec<(&'static str, String)> {
    let realtime_resource = if ir.realtime.enabled {
        include_str!("../templates/web/realtime/useRealtimeResource.ts")
    } else {
        include_str!("../templates/web/realtime/useRealtimeResourceDisabled.ts")
    };
    let resource_detail = include_str!("../templates/web/ResourceDetail.tsx")
        .replace(
            "__AUDIT_IMPORT__",
            if ir.audit.enabled {
                "import { RecordHistory } from \"../audit/RecordHistory\";"
            } else {
                ""
            },
        )
        .replace(
            "__RECORD_HISTORY__",
            if ir.audit.enabled {
                "          {id && <RecordHistory entity={resource.id} recordId={id} />}"
            } else {
                ""
            },
        )
        .replace(
            "__ACTIVITY_IMPORT__",
            if ir.activity.enabled {
                "import { ActivityTimeline } from \"../activity/ActivityTimeline\";"
            } else {
                ""
            },
        )
        .replace(
            "__ACTIVITY_TIMELINE__",
            if ir.activity.enabled {
                "          {id && resource.activity && <ActivityTimeline resource={resource} recordId={id} />}"
            } else {
                ""
            },
        );
    vec![
        (
            "web/src/pages/ResourceFilters.tsx",
            include_str!("../templates/web/ResourceFilters.tsx").to_owned(),
        ),
        (
            "web/src/pages/resource-list/BulkActions.tsx",
            include_str!("../templates/web/pages/resource-list/BulkActions.tsx").to_owned(),
        ),
        (
            "web/src/pages/resource-list/ResourceTable.tsx",
            include_str!("../templates/web/pages/resource-list/ResourceTable.tsx").to_owned(),
        ),
        (
            "web/src/pages/resource-list/ResourceInsights.tsx",
            include_str!("../templates/web/pages/resource-list/ResourceInsights.tsx").to_owned(),
        ),
        (
            "web/src/pages/resource-list/InlineEditor.tsx",
            include_str!("../templates/web/pages/resource-list/InlineEditor.tsx").to_owned(),
        ),
        (
            "web/src/pages/resource-list/SavedViews.tsx",
            include_str!("../templates/web/pages/resource-list/SavedViews.tsx").to_owned(),
        ),
        (
            "web/src/pages/resource-list/ViewOptions.tsx",
            include_str!("../templates/web/pages/resource-list/ViewOptions.tsx").to_owned(),
        ),
        (
            "web/src/pages/resource-list/useCsvTransfer.ts",
            include_str!("../templates/web/pages/resource-list/useCsvTransfer.ts").to_owned(),
        ),
        (
            "web/src/pages/ResourceForm.tsx",
            include_str!("../templates/web/ResourceForm.tsx").to_owned(),
        ),
        (
            "web/src/pages/WorkflowActions.tsx",
            include_str!("../templates/web/WorkflowActions.tsx").to_owned(),
        ),
        ("web/src/pages/ResourceDetail.tsx", resource_detail),
        (
            "web/src/realtime/useRealtimeResource.ts",
            realtime_resource.to_owned(),
        ),
        (
            "web/src/styles.css",
            include_str!("../templates/web/styles.css").to_owned(),
        ),
        (
            "web/package.json",
            include_str!("../templates/web/package.json").to_owned(),
        ),
        (
            "web/pnpm-lock.yaml",
            include_str!("../templates/web/pnpm-lock.yaml").to_owned(),
        ),
    ]
}

fn extend_module_artifacts(ir: &AppIr, artifacts: &mut Vec<Artifact>) {
    let title = app_display_name(&ir.app.name);
    if ir.auth.enabled {
        artifacts.extend([
            Artifact::text(
                "web/src/auth/Auth.tsx",
                include_str!("../templates/web/auth/Auth.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/AuthPages.tsx",
                with_app_title(include_str!("../templates/web/auth/AuthPages.tsx"), &title),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/AdminStoragePages.tsx",
                include_str!("../templates/web/auth/AdminStoragePages.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/AdminSchedulesPage.tsx",
                include_str!("../templates/web/auth/AdminSchedulesPage.tsx"),
                ArtifactKind::Web,
            ),
        ]);
    }
    if ir.tenant.enabled {
        artifacts.push(Artifact::text(
            "web/src/tenant/Tenant.tsx",
            with_app_title(include_str!("../templates/web/tenant/Tenant.tsx"), &title),
            ArtifactKind::Web,
        ));
    }
    if ir.audit.enabled {
        artifacts.extend([
            Artifact::text(
                "web/src/audit/AuditPage.tsx",
                include_str!("../templates/web/audit/AuditPage.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/audit/RecordHistory.tsx",
                include_str!("../templates/web/audit/RecordHistory.tsx"),
                ArtifactKind::Web,
            ),
        ]);
    }
    if ir.report.enabled {
        artifacts.push(Artifact::text(
            "web/src/report/ReportPage.tsx",
            include_str!("../templates/web/report/ReportPage.tsx"),
            ArtifactKind::Web,
        ));
    }
    if ir.activity.enabled {
        let activity_realtime = if ir.realtime.enabled {
            include_str!("../templates/web/activity/useActivityRealtime.ts")
        } else {
            include_str!("../templates/web/activity/useActivityRealtimeDisabled.ts")
        };
        artifacts.extend([
            Artifact::text(
                "web/src/activity/ActivityTimeline.tsx",
                include_str!("../templates/web/activity/ActivityTimeline.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/activity/useActivityRealtime.ts",
                activity_realtime,
                ArtifactKind::Web,
            ),
        ]);
    }
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
    "paths": {
      "react": ["./node_modules/@types/react/index.d.ts"],
      "react/*": ["./node_modules/@types/react/*"]
    },
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
    r#"import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { react: new URL("./node_modules/react", import.meta.url).pathname },
  },
  server: { host: "127.0.0.1", port: 5173 },
  test: { environment: "happy-dom" },
});
"#
}

fn registry_source(ir: &AppIr) -> String {
    let field_components = ir
        .entities
        .iter()
        .flat_map(|entity| &entity.fields)
        .filter_map(|field| field.ui_component.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    let field_members = field_components
        .iter()
        .map(|component| format!("    {component}: ComponentType<FieldComponentProps>;"))
        .collect::<Vec<_>>()
        .join("\n");
    let page_components = ir
        .pages
        .iter()
        .map(|page| page.component.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let page_members = page_components
        .iter()
        .map(|component| format!("    {component}: ComponentType<PageComponentProps>;"))
        .collect::<Vec<_>>()
        .join("\n");
    let component_import = if field_components.is_empty() && page_components.is_empty() {
        ""
    } else {
        "import type { ComponentType } from \"react\";\n\n"
    };
    let field_registry = if field_members.is_empty() {
        "Record<string, never>".to_owned()
    } else {
        format!("{{\n{field_members}\n  }}")
    };
    let page_registry = if page_members.is_empty() {
        "Record<string, never>".to_owned()
    } else {
        format!("{{\n{page_members}\n  }}")
    };
    let pages = ir
        .pages
        .iter()
        .map(|page| {
            format!(
                "  {{ name: {:?}, label: {:?}, path: {:?}, component: {:?} }},",
                page.rust_name, page.label, page.path, page.component
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{}{component_import}import type {{ ResourceDefinition }} from \"../resource\";\n\nexport interface FieldComponentProps {{\n  label: string;\n  required: boolean;\n  value: string | boolean | undefined;\n  error?: string;\n  readOnly: boolean;\n  onChange(value: string | boolean): void;\n}}\n\nexport interface PageComponentProps {{\n  resources: readonly ResourceDefinition[];\n}}\n\nexport interface AppStructRegistry {{\n  fields: {field_registry};\n  pages: {page_registry};\n}}\n\nexport interface CustomPageDefinition {{\n  name: string;\n  label: string;\n  path: string;\n  component: keyof AppStructRegistry[\"pages\"];\n}}\n\nexport function defineAppStructRegistry<T extends AppStructRegistry>(registry: T): T {{ return registry; }}\n\nexport const customPages: readonly CustomPageDefinition[] = [\n{pages}\n];\n",
        generated_header("//"),
    )
}

pub(super) fn app_display_name(name: &str) -> String {
    name.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                format!("{}{}", first.to_uppercase(), characters.as_str())
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn with_app_title(source: &str, title: &str) -> String {
    source.replace("__APP_TITLE__", title)
}

#[cfg(test)]
mod tests {
    use super::{app_display_name, with_app_title};

    #[test]
    fn app_display_name_title_cases_hyphenated_package_names() {
        assert_eq!(app_display_name("notes"), "Notes");
        assert_eq!(app_display_name("project-manager"), "Project Manager");
    }

    #[test]
    fn with_app_title_replaces_the_placeholder() {
        assert_eq!(
            with_app_title("<title>__APP_TITLE__</title>", "Notes"),
            "<title>Notes</title>"
        );
    }
}
