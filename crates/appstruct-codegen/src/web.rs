mod resources;

use crate::{Artifact, ArtifactKind, generated_header};
use appstruct_ir::AppIr;

pub(crate) fn plan(ir: &AppIr) -> Vec<Artifact> {
    let main = if requires_registry(ir) {
        include_str!("../templates/web/main_with_registry.tsx")
    } else {
        include_str!("../templates/web/main.tsx")
    };
    let app = if ir.tenant.enabled && ir.audit.enabled {
        include_str!("../templates/web/AppTenantAudit.tsx")
    } else if ir.tenant.enabled {
        include_str!("../templates/web/AppTenant.tsx")
    } else if ir.audit.enabled {
        include_str!("../templates/web/AppAudit.tsx")
    } else if ir.auth.enabled {
        include_str!("../templates/web/AppAuth.tsx")
    } else {
        include_str!("../templates/web/App.tsx")
    };
    let layout = if ir.tenant.enabled && ir.audit.enabled {
        include_str!("../templates/web/LayoutTenantAudit.tsx")
    } else if ir.tenant.enabled {
        include_str!("../templates/web/LayoutTenant.tsx")
    } else if ir.audit.enabled {
        include_str!("../templates/web/LayoutAudit.tsx")
    } else if ir.auth.enabled {
        include_str!("../templates/web/LayoutAuth.tsx")
    } else {
        include_str!("../templates/web/Layout.tsx")
    };
    let static_files = vec![
        (
            "web/index.html",
            include_str!("../templates/web/index.html"),
        ),
        ("web/.gitignore", include_str!("../templates/web/gitignore")),
        (
            "web/eslint.config.js",
            include_str!("../templates/web/eslint.config.js"),
        ),
        ("web/src/main.tsx", main),
        (
            "web/src/resource.ts",
            include_str!("../templates/web/resource.ts"),
        ),
        (
            "web/src/query.ts",
            include_str!("../templates/web/query.ts"),
        ),
        (
            "web/src/navigation.tsx",
            include_str!("../templates/web/navigation.tsx"),
        ),
        (
            "web/src/framework.test.ts",
            include_str!("../templates/web/framework.test.ts"),
        ),
        ("web/src/app/App.tsx", app),
        (
            "web/src/app/ResourceRoutes.tsx",
            include_str!("../templates/web/ResourceRoutes.tsx"),
        ),
        ("web/src/app/Layout.tsx", layout),
        (
            "web/src/pages/ResourceList.tsx",
            include_str!("../templates/web/ResourceList.tsx"),
        ),
        (
            "web/src/pages/ResourceFilters.tsx",
            include_str!("../templates/web/ResourceFilters.tsx"),
        ),
        (
            "web/src/pages/ResourceForm.tsx",
            include_str!("../templates/web/ResourceForm.tsx"),
        ),
        (
            "web/src/pages/ResourceDetail.tsx",
            include_str!("../templates/web/ResourceDetail.tsx"),
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

fn extend_module_artifacts(ir: &AppIr, artifacts: &mut Vec<Artifact>) {
    if ir.auth.enabled {
        artifacts.extend([
            Artifact::text(
                "web/src/auth/Auth.tsx",
                include_str!("../templates/web/auth/Auth.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/AuthPages.tsx",
                include_str!("../templates/web/auth/AuthPages.tsx"),
                ArtifactKind::Web,
            ),
        ]);
    }
    if ir.tenant.enabled {
        artifacts.push(Artifact::text(
            "web/src/tenant/Tenant.tsx",
            include_str!("../templates/web/tenant/Tenant.tsx"),
            ArtifactKind::Web,
        ));
    }
    if ir.audit.enabled {
        artifacts.push(Artifact::text(
            "web/src/audit/AuditPage.tsx",
            include_str!("../templates/web/audit/AuditPage.tsx"),
            ArtifactKind::Web,
        ));
    }
}

fn requires_registry(ir: &AppIr) -> bool {
    !ir.pages.is_empty()
        || ir
            .entities
            .iter()
            .flat_map(|entity| &entity.fields)
            .any(|field| field.ui_component.is_some())
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
    r#"import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { react: new URL("./node_modules/react", import.meta.url).pathname },
  },
  server: { host: "127.0.0.1", port: 5173 },
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
        "{}{component_import}export interface FieldComponentProps {{\n  label: string;\n  required: boolean;\n  value: string | boolean | undefined;\n  error?: string;\n  readOnly: boolean;\n  onChange(value: string | boolean): void;\n}}\n\nexport type PageComponentProps = Record<string, never>;\n\nexport interface AppStructRegistry {{\n  fields: {field_registry};\n  pages: {page_registry};\n}}\n\nexport interface CustomPageDefinition {{\n  name: string;\n  label: string;\n  path: string;\n  component: keyof AppStructRegistry[\"pages\"];\n}}\n\nexport function defineAppStructRegistry<T extends AppStructRegistry>(registry: T): T {{ return registry; }}\n\nexport const customPages: readonly CustomPageDefinition[] = [\n{pages}\n];\n",
        generated_header("//"),
    )
}
