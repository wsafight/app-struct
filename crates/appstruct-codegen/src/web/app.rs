use appstruct_ir::AppIr;

pub(super) fn source(ir: &AppIr) -> String {
    if !ir.auth.enabled {
        return include_str!("../../templates/web/App.tsx").to_owned();
    }
    let audit = ir.audit.enabled;
    let tenant = ir.tenant.enabled;
    include_str!("../../templates/web/AppAuthenticated.tsx")
        .replace(
            "__REACT_IMPORT__",
            if audit {
                "import { lazy, useState, type ComponentType } from \"react\";"
            } else {
                "import { useState, type ComponentType } from \"react\";"
            },
        )
        .replace(
            "__RESOURCE_IMPORT__",
            if audit {
                "import { auditAccess, resources } from \"../generated/resources\";"
            } else {
                "import { resources } from \"../generated/resources\";"
            },
        )
        .replace(
            "__RESOURCE_ACCESS_IMPORT__",
            if audit {
                "import { useCanAccessRule, useVisibleResources } from \"../resource\";"
            } else {
                "import { useVisibleResources } from \"../resource\";"
            },
        )
        .replace(
            "__TENANT_IMPORT__",
            if tenant {
                "import { InvitationAcceptPage, OrganizationPage, RequireTenant, TenantProvider } from \"../tenant/Tenant\";"
            } else {
                ""
            },
        )
        .replace(
            "__AUDIT_PAGE__",
            if audit {
                "const AuditPage = lazy(() => import(\"../audit/AuditPage\").then(({ AuditPage: component }) => ({ default: component })));\n"
            } else {
                ""
            },
        )
        .replace(
            "__TENANT_ROOT__",
            if tenant {
                "function TenantRoot() {\n  return <TenantProvider><RequireTenant /></TenantProvider>;\n}\n"
            } else {
                ""
            },
        )
        .replace(
            "__INVITATION_ROUTE__",
            if tenant {
                "    { path: \"/accept-invitation\", component: InvitationAcceptPage },"
            } else {
                ""
            },
        )
        .replace(
            "__AUTHENTICATED_SCOPE__",
            if tenant {
                "{ id: \"_tenant\", component: TenantRoot, children: [layout] }"
            } else {
                "layout"
            },
        )
        .replace(
            "__AUDIT_ROUTE__",
            if audit {
                "    { path: \"/audit\", component: AuditPage },"
            } else {
                ""
            },
        )
        .replace(
            "__ORGANIZATION_ROUTE__",
            if tenant {
                "    { path: \"/organization\", component: OrganizationPage },"
            } else {
                ""
            },
        )
        .replace(
            "__HOME_REDIRECT__",
            if audit {
                "  const canReadAudit = useCanAccessRule(auditAccess);\n  return <Navigate to={`/${first?.slug ?? customPages[0]?.path ?? (canReadAudit ? \"audit\" : \"empty\")}`} replace />;"
            } else {
                "  return <Navigate to={`/${first?.slug ?? customPages[0]?.path ?? \"empty\"}`} replace />;"
            },
        )
}

pub(super) fn layout_source(ir: &AppIr) -> String {
    if !ir.auth.enabled {
        return include_str!("../../templates/web/Layout.tsx").to_owned();
    }
    let audit = ir.audit.enabled;
    let tenant = ir.tenant.enabled;
    include_str!("../../templates/web/LayoutAuthenticated.tsx")
        .replace("__HISTORY_ICON__", if audit { ", History" } else { "" })
        .replace(
            "__AUDIT_RESOURCE_IMPORT__",
            if audit {
                "import { auditAccess } from \"../generated/resources\";"
            } else {
                ""
            },
        )
        .replace(
            "__RESOURCE_HOOKS__",
            if audit {
                "useCanAccessRule, useVisibleResources"
            } else {
                "useVisibleResources"
            },
        )
        .replace(
            "__TENANT_IMPORT__",
            if tenant {
                "import { TenantSwitcher } from \"../tenant/Tenant\";"
            } else {
                ""
            },
        )
        .replace(
            "__AUDIT_ACCESS__",
            if audit {
                "  const canReadAudit = useCanAccessRule(auditAccess);"
            } else {
                ""
            },
        )
        .replace(
            "__TENANT_SWITCHER__",
            if tenant {
                "      <TenantSwitcher />"
            } else {
                ""
            },
        )
        .replace(
            "__AUDIT_LINK__",
            if audit {
                "        {canReadAudit && <NavLink to=\"/audit\"><History size={15} /> Audit log</NavLink>}"
            } else {
                ""
            },
        )
}
