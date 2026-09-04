import {
  type QueryClient,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useRef,
} from "react";
import { Outlet, useLocation, useNavigate } from "../navigation";
import { authApi, sessionSyncKey, type AuthUser } from "../generated/client";
import { appQueryKeys } from "../query";
import { ResourceActorProvider } from "../resource";

interface AuthContextValue {
  loading: boolean;
  user: AuthUser | null;
  login(email: string, password: string): Promise<void>;
  register(email: string, password: string): Promise<void>;
  logout(): Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

function clearPrivateQueries(queryClient: QueryClient) {
  queryClient.removeQueries({
    predicate: (query) => query.queryKey[0] !== appQueryKeys.session[0],
  });
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const session = useQuery<AuthUser | null>({
    queryKey: appQueryKeys.session,
    queryFn: ({ signal }) => authApi.me({ signal }),
    staleTime: 5 * 60_000,
  });
  const loading = session.isPending;
  const user = session.data ?? null;

  useEffect(() => {
    const handleUnauthorized = () => {
      void queryClient.cancelQueries();
      clearPrivateQueries(queryClient);
      queryClient.setQueryData(appQueryKeys.session, null);
    };
    const handleStorage = (event: StorageEvent) => {
      if (event.key === sessionSyncKey) {
        void queryClient.cancelQueries();
        clearPrivateQueries(queryClient);
        void queryClient.invalidateQueries({ queryKey: appQueryKeys.session });
      }
    };
    window.addEventListener("appstruct:unauthorized", handleUnauthorized);
    window.addEventListener("storage", handleStorage);
    return () => {
      window.removeEventListener("appstruct:unauthorized", handleUnauthorized);
      window.removeEventListener("storage", handleStorage);
    };
  }, [queryClient]);

  const value = useMemo<AuthContextValue>(
    () => ({
      loading,
      user,
      async login(email, password) {
        const nextUser = await authApi.login(email, password);
        await queryClient.cancelQueries();
        clearPrivateQueries(queryClient);
        queryClient.setQueryData(appQueryKeys.session, nextUser);
      },
      async register(email, password) {
        const nextUser = await authApi.register(email, password);
        await queryClient.cancelQueries();
        clearPrivateQueries(queryClient);
        queryClient.setQueryData(appQueryKeys.session, nextUser);
      },
      async logout() {
        await authApi.logout();
        await queryClient.cancelQueries();
        clearPrivateQueries(queryClient);
        queryClient.setQueryData(appQueryKeys.session, null);
      },
    }),
    [loading, queryClient, user],
  );

  return (
    <AuthContext.Provider value={value}>
      <ResourceActorProvider user={user}>{children}</ResourceActorProvider>
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextValue {
  const value = useContext(AuthContext);
  if (!value) throw new Error("AuthProvider is missing");
  return value;
}

export function RequireAuth() {
  const { loading, user } = useAuth();
  const location = useLocation();
  const navigate = useNavigate();
  const redirecting = useRef(false);

  useEffect(() => {
    if (loading || user) {
      redirecting.current = false;
      return;
    }
    if (redirecting.current) return;
    redirecting.current = true;
    void navigate("/login", {
      replace: true,
      state: {
        from: {
          pathname: location.pathname,
          searchStr: location.searchStr,
          hash: location.hash,
        },
      },
    });
  }, [
    loading,
    location.hash,
    location.pathname,
    location.searchStr,
    navigate,
    user,
  ]);

  if (loading || !user)
    return <div className="auth-loading" aria-label="Loading" />;
  return <Outlet />;
}
