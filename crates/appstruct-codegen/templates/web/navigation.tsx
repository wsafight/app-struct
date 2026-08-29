import {
  Link as RouterLink,
  Navigate as RouterNavigate,
  Outlet,
  RouterProvider,
  createRootRoute,
  createRoute,
  createRouter,
  useNavigate as useRouterNavigate,
  useParams as useRouterParams,
  useRouterState,
  type AnyRoute,
  type AnyRouter,
} from "@tanstack/react-router";
import {
  type AnchorHTMLAttributes,
  type ComponentType,
  Suspense,
  useCallback,
  useMemo,
} from "react";

interface LinkProps extends Omit<AnchorHTMLAttributes<HTMLAnchorElement>, "href"> {
  to: string;
  replace?: boolean;
  state?: unknown;
}

const UntypedRouterLink = RouterLink as ComponentType<LinkProps & { activeProps?: { className?: string } }>;

export function Link({ to, ...props }: LinkProps) {
  return <UntypedRouterLink to={to} {...props} />;
}

export function NavLink({ to, className, ...props }: LinkProps) {
  const activeClassName = [className, "active"].filter(Boolean).join(" ");
  return <UntypedRouterLink to={to} className={className} activeProps={{ className: activeClassName }} {...props} />;
}

export function Navigate({ to, replace, state }: { to: string; replace?: boolean; state?: unknown }) {
  return <RouterNavigate to={to as never} replace={replace} state={state as never} />;
}

export function useNavigate() {
  const navigate = useRouterNavigate();
  return useCallback(
    (to: string, options?: { replace?: boolean; state?: unknown }) =>
      navigate({ to: to as never, replace: options?.replace, state: options?.state as never }),
    [navigate],
  );
}

export function useParams(): Record<string, string | undefined> {
  return useRouterParams({ strict: false }) as Record<string, string | undefined>;
}

export function useLocation() {
  return useRouterState({ select: (state) => state.location });
}

type SearchParamsInput = URLSearchParams | ((current: URLSearchParams) => URLSearchParams);

export function useSearchParams(): [URLSearchParams, (next: SearchParamsInput, options?: { replace?: boolean }) => void] {
  const navigate = useRouterNavigate();
  const location = useRouterState({ select: (state) => state.location });
  const searchParams = useMemo(() => new URLSearchParams(location.searchStr), [location.searchStr]);
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
  children?: RuntimeRoute[];
}

export function createRuntimeRouter(component: ComponentType, routes: RuntimeRoute[]): AnyRouter {
  const rootRoute = createRootRoute({ component: component as never });
  const routeTree = rootRoute.addChildren(routes.map((route) => createRuntimeRoute(rootRoute, route)));
  return createRouter({ routeTree, defaultPreload: "intent" });
}

function createRuntimeRoute(parent: AnyRoute, definition: RuntimeRoute): AnyRoute {
  const route = createRoute({
    getParentRoute: () => parent,
    ...(definition.path === undefined ? { id: definition.id! } : { path: definition.path }),
    component: definition.component,
  } as never) as AnyRoute;
  return definition.children?.length
    ? (route.addChildren(definition.children.map((child) => createRuntimeRoute(route, child))) as AnyRoute)
    : route;
}

export function RuntimeRouter({ router }: { router: AnyRouter }) {
  return <Suspense fallback={<div className="auth-loading" aria-label="Loading" />}><RouterProvider router={router} /></Suspense>;
}

export { Outlet };
