import { FormEvent, ReactNode, useState } from "react";
import { KeyRound, LogIn, Mail, UserPlus } from "lucide-react";
import { Link, Navigate, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { authApi, authFeatures } from "../generated/client";
import { useAuth } from "./Auth";

export function LoginPage() {
  return <CredentialsPage mode="login" />;
}

export function RegisterPage() {
  if (!authFeatures.registration) return <Navigate to="/login" replace />;
  return <CredentialsPage mode="register" />;
}

function CredentialsPage({ mode }: { mode: "login" | "register" }) {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);
  if (auth.user) return <Navigate to="/" replace />;

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSubmitting(true);
    setError("");
    try {
      if (mode === "login") await auth.login(email, password); else await auth.register(email, password);
      const from = (location.state as { from?: { pathname?: string } } | null)?.from?.pathname ?? "/";
      navigate(from, { replace: true });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "The request could not be completed");
    } finally {
      setSubmitting(false);
    }
  }

  const registering = mode === "register";
  return <AuthFrame title={registering ? "Create account" : "Sign in"}>
    <form className="auth-form" onSubmit={(event) => void submit(event)}>
      {error && <div className="alert" role="alert">{error}</div>}
      <label>Email<input type="email" autoComplete="email" value={email} onChange={(event) => setEmail(event.target.value)} required /></label>
      <label>Password<input type="password" minLength={12} autoComplete={registering ? "new-password" : "current-password"} value={password} onChange={(event) => setPassword(event.target.value)} required /></label>
      <button className="primary-button" disabled={submitting}>{registering ? <UserPlus size={17} /> : <LogIn size={17} />}{submitting ? "Working..." : registering ? "Create account" : "Sign in"}</button>
      <div className="auth-links">
        {authFeatures.passwordReset && <Link to="/forgot-password">Forgot password?</Link>}
        {authFeatures.registration && <Link to={registering ? "/login" : "/register"}>{registering ? "Sign in" : "Create account"}</Link>}
      </div>
    </form>
  </AuthFrame>;
}

export function ForgotPasswordPage() {
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState("");
  if (!authFeatures.passwordReset) return <Navigate to="/login" replace />;
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    try { await authApi.requestPasswordReset(email); setSent(true); }
    catch (reason) { setError(reason instanceof Error ? reason.message : "The request could not be completed"); }
  }
  return <AuthFrame title="Reset password">{sent ? <div className="auth-success"><Mail size={20} /> Check your email</div> : <form className="auth-form" onSubmit={(event) => void submit(event)}>{error && <div className="alert" role="alert">{error}</div>}<label>Email<input type="email" autoComplete="email" value={email} onChange={(event) => setEmail(event.target.value)} required /></label><button className="primary-button"><Mail size={17} /> Send reset link</button><div className="auth-links"><Link to="/login">Back to sign in</Link></div></form>}</AuthFrame>;
}

export function ResetPasswordPage() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const token = params.get("token") ?? "";
  if (!authFeatures.passwordReset) return <Navigate to="/login" replace />;
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    try { await authApi.resetPassword(token, password); navigate("/login", { replace: true }); }
    catch (reason) { setError(reason instanceof Error ? reason.message : "The request could not be completed"); }
  }
  return <AuthFrame title="Choose a password"><form className="auth-form" onSubmit={(event) => void submit(event)}>{error && <div className="alert" role="alert">{error}</div>}<label>New password<input type="password" minLength={12} autoComplete="new-password" value={password} onChange={(event) => setPassword(event.target.value)} required /></label><button className="primary-button" disabled={!token}><KeyRound size={17} /> Update password</button></form></AuthFrame>;
}

function AuthFrame({ title, children }: { title: string; children: ReactNode }) {
  return <main className="auth-page"><section className="auth-panel"><div className="auth-brand">AppStruct</div><h1>{title}</h1>{children}</section></main>;
}
