import { createContext, ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { Navigate, Outlet, useLocation } from "react-router-dom";
import { authApi, type AuthUser } from "../generated/client";

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
    authApi.me().then(setUser).catch(() => setUser(null)).finally(() => setLoading(false));
  }, []);

  const value = useMemo<AuthContextValue>(() => ({
    loading,
    user,
    async login(email, password) { setUser(await authApi.login(email, password)); },
    async register(email, password) { setUser(await authApi.register(email, password)); },
    async logout() { await authApi.logout(); setUser(null); },
  }), [loading, user]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
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
