import {
  Link as RouterLink,
  Navigate as RouterNavigate,
  Outlet,
  RouterProvider,
  createRootRoute,
  createRoute,
  createRouter,
  useNavigate as useRouterNavigate,
  useBlocker as useRouterBlocker,
  useParams as useRouterParams,
  useRouterState,
  type AnyRoute,
  type AnyRouter,
} from "@tanstack/react-router";
import {
  Component,
  type ErrorInfo,
  type AnchorHTMLAttributes,
  type ComponentType,
  type ReactNode,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
} from "react";

interface LinkProps extends Omit<
  AnchorHTMLAttributes<HTMLAnchorElement>,
  "href"
> {
  to: string;
  replace?: boolean;
  state?: unknown;
}

const UntypedRouterLink = RouterLink as ComponentType<
  LinkProps & { activeProps?: { className?: string } }
>;

export function Link({ to, ...props }: LinkProps) {
  return <UntypedRouterLink to={to} {...props} />;
}

export function NavLink({ to, className, ...props }: LinkProps) {
  const activeClassName = [className, "active"].filter(Boolean).join(" ");
  return (
    <UntypedRouterLink
      to={to}
      className={className}
      activeProps={{ className: activeClassName }}
      {...props}
    />
  );
}

export function Navigate({
  to,
  replace,
  state,
}: {
  to: string;
  replace?: boolean;
  state?: unknown;
}) {
  return (
    <RouterNavigate to={to as never} replace={replace} state={state as never} />
  );
}

export function useNavigate() {
  const navigate = useRouterNavigate();
  return useCallback(
    (to: string, options?: { replace?: boolean; state?: unknown }) =>
      navigate({
        to: to as never,
        replace: options?.replace,
        state: options?.state as never,
      }),
    [navigate],
  );
}

export function useParams(): Record<string, string | undefined> {
  return useRouterParams({ strict: false }) as Record<
    string,
    string | undefined
  >;
}

export function useLocation() {
  return useRouterState({ select: (state) => state.location });
}

export function useUnsavedChanges(enabled: boolean) {
  const blocker = useRouterBlocker({
    shouldBlockFn: () => enabled,
    enableBeforeUnload: enabled,
    withResolver: true,
  });
  useEffect(() => {
    if (blocker.status !== "blocked") return;
    if (window.confirm("Discard unsaved changes?")) blocker.proceed();
    else blocker.reset();
  }, [blocker]);
}

type SearchParamsInput =
  URLSearchParams | ((current: URLSearchParams) => URLSearchParams);

export function useSearchParams(): [
  URLSearchParams,
  (next: SearchParamsInput, options?: { replace?: boolean }) => void,
] {
  const navigate = useRouterNavigate();
  const location = useRouterState({ select: (state) => state.location });
  const searchParams = useMemo(
    () => new URLSearchParams(location.searchStr),
    [location.searchStr],
  );
  const setSearchParams = useCallback(
    (next: SearchParamsInput, options?: { replace?: boolean }) => {
      const current = new URLSearchParams(location.searchStr);
      const resolved = typeof next === "function" ? next(current) : next;
      void navigate({
        to: location.pathname as never,
        search: Object.fromEntries(resolved.entries()) as never,
        replace: options?.replace,
      });
    },
    [location.pathname, location.searchStr, navigate],
  );
  return [searchParams, setSearchParams];
}

export interface RuntimeRoute {
  id?: string;
  path?: string;
  component: ComponentType;
  validateSearch?: (search: Record<string, unknown>) => Record<string, unknown>;
  children?: RuntimeRoute[];
}

export interface ResourceSearch {
  page?: number;
  page_size?: number;
  sort?: string;
  q?: string;
  trash?: "1";
  [key: string]: string | number | undefined;
}

export function validateResourceSearch(
  search: Record<string, unknown>,
): ResourceSearch {
  const result: ResourceSearch = {};
  const page = searchInteger(search.page, 1, 10_000);
  const pageSize = searchInteger(search.page_size, 1, 100);
  if (page !== undefined && page !== 1) result.page = page;
  if (pageSize !== undefined && pageSize !== 25) result.page_size = pageSize;
  for (const key of ["sort", "q"] as const) {
    if (typeof search[key] === "string" && search[key])
      result[key] = search[key];
  }
  if (search.trash === "1" || search.trash === 1) result.trash = "1";
  for (const [key, value] of Object.entries(search)) {
    if (
      /^filter\[\w+\](?:\[(?:gte|lte)\])?$/.test(key) &&
      typeof value === "string" &&
      value
    )
      result[key] = value;
  }
  return result;
}

function searchInteger(
  value: unknown,
  minimum: number,
  maximum = Number.MAX_SAFE_INTEGER,
): number | undefined {
  const parsed =
    typeof value === "number"
      ? value
      : typeof value === "string"
        ? Number(value)
        : Number.NaN;
  return Number.isInteger(parsed) && parsed >= minimum && parsed <= maximum
    ? parsed
    : undefined;
}

export function createRuntimeRouter(
  component: ComponentType,
  routes: RuntimeRoute[],
): AnyRouter {
  const rootRoute = createRootRoute({
    component: component as never,
    errorComponent: RouteErrorPage as never,
    notFoundComponent: NotFoundPage,
  });
  const routeTree = rootRoute.addChildren(
    routes.map((route) => createRuntimeRoute(rootRoute, route)),
  );
  return createRouter({ routeTree, defaultPreload: "intent" });
}

function createRuntimeRoute(
  parent: AnyRoute,
  definition: RuntimeRoute,
): AnyRoute {
  const route = createRoute({
    getParentRoute: () => parent,
    ...(definition.path === undefined
      ? { id: definition.id! }
      : { path: definition.path }),
    component: definition.component,
    ...(definition.validateSearch
      ? { validateSearch: definition.validateSearch }
      : {}),
  } as never) as AnyRoute;
  return definition.children?.length
    ? (route.addChildren(
        definition.children.map((child) => createRuntimeRoute(route, child)),
      ) as AnyRoute)
    : route;
}

export function RuntimeRouter({ router }: { router: AnyRouter }) {
  return (
    <AppErrorBoundary>
      <Suspense
        fallback={<div className="auth-loading" aria-label="Loading" />}
      >
        <RouterProvider router={router} />
      </Suspense>
    </AppErrorBoundary>
  );
}

class AppErrorBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Application render failed", error, info.componentStack);
  }

  render() {
    return this.state.failed ? (
      <RecoveryPage title="Application unavailable" />
    ) : (
      this.props.children
    );
  }
}

function RouteErrorPage() {
  return <RecoveryPage title="Page unavailable" />;
}

function NotFoundPage() {
  return (
    <main className="auth-page">
      <section className="auth-panel">
        <h1>Page not found</h1>
        <a className="primary-button" href="/">
          Go home
        </a>
      </section>
    </main>
  );
}

function RecoveryPage({ title }: { title: string }) {
  return (
    <main className="auth-page">
      <section className="auth-panel">
        <h1>{title}</h1>
        <button
          className="primary-button"
          type="button"
          onClick={() => window.location.reload()}
        >
          Reload
        </button>
      </section>
    </main>
  );
}

export { Outlet };
