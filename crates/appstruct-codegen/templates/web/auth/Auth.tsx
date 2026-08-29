import { createContext, ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { Navigate, Outlet, useLocation } from "../navigation";
import { authApi, type AuthUser } from "../generated/client";
import { queryClient } from "../query";
import { ResourceActorProvider } from "../resource";

interface AuthContextValue {
  loading: boolean;
  user: AuthUser | null;
  login(email: string, password: string): Promise<void>;
  register(email: string, password: string): Promise<void>;
  logout(): Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [loading, setLoading] = useState(true);
  const [user, setUser] = useState<AuthUser | null>(null);

  useEffect(() => {
    const handleUnauthorized = () => {
      queryClient.clear();
      setUser(null);
      setLoading(false);
    };
    window.addEventListener("appstruct:unauthorized", handleUnauthorized);
    return () => window.removeEventListener("appstruct:unauthorized", handleUnauthorized);
  }, []);

  useEffect(() => {
    authApi.me().then(setUser).catch(() => setUser(null)).finally(() => setLoading(false));
  }, []);

  const value = useMemo<AuthContextValue>(() => ({
    loading,
    user,
    async login(email, password) { queryClient.clear(); setUser(await authApi.login(email, password)); },
    async register(email, password) { queryClient.clear(); setUser(await authApi.register(email, password)); },
    async logout() { await authApi.logout(); queryClient.clear(); setUser(null); },
  }), [loading, user]);

  return <AuthContext.Provider value={value}><ResourceActorProvider user={user}>{children}</ResourceActorProvider></AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const value = useContext(AuthContext);
  if (!value) throw new Error("AuthProvider is missing");
  return value;
}

export function RequireAuth() {
  const { loading, user } = useAuth();
  const location = useLocation();
  if (loading) return <div className="auth-loading" aria-label="Loading" />;
  return user ? <Outlet /> : <Navigate to="/login" state={{ from: location }} replace />;
}
