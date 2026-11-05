import type { ComponentType } from "react";
import {
  customPages,
  type AppStructRegistry,
  type PageComponentProps,
  type PageScope,
  workspacePath,
} from "../generated/registry";
import { resources } from "../generated/resources";
import { Link, Outlet, type RuntimeRoute } from "../navigation";
import { t } from "../ui";

export const workspacePages = customPages.filter(
  (page) => page.scope === "workspace",
);

export function customPageRoutes(
  registry: AppStructRegistry | undefined,
  scope: PageScope,
): RuntimeRoute[] {
  const components = registry?.pages as
    Record<string, ComponentType<PageComponentProps>> | undefined;
  return customPages
    .filter((page) => page.scope === scope)
    .map((page) => {
      const Component = components?.[String(page.component)];
      return {
        path: `/${page.path.replace(/^\/+/, "")}`,
        component: Component
          ? () => <Component resources={resources} />
          : () => (
              <main className="page">
                <div className="alert" role="alert">
                  {t("Page renderer unavailable")}
                </div>
              </main>
            ),
      };
    });
}

export function PageScopeLayout({
  registry,
  scope,
}: {
  registry?: AppStructRegistry;
  scope: PageScope;
}) {
  const pages = customPages.filter((page) => page.scope === scope);
  const Component = registry?.layouts?.[scope];
  if (Component)
    return (
      <Component resources={resources} scope={scope} pages={pages}>
        <Outlet />
      </Component>
    );
  return (
    <div className="application-shell">
      <header className="application-header">
        <Link to="/" className="application-brand">
          __APP_TITLE__
        </Link>
        <nav aria-label={t("Navigation")}>
          {pages.map((page) => (
            <Link key={page.name} to={`/${page.path.replace(/^\/+/, "")}`}>
              {t(page.label)}
            </Link>
          ))}
          __AUTH_LINK__
        </nav>
      </header>
      <Outlet />
    </div>
  );
}
