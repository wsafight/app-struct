import { errorMessage } from "../resource";
import { t } from "../ui";
import {
  type FormEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { KeyRound, LogIn, Mail, UserPlus } from "lucide-react";
import {
  Link,
  Navigate,
  useLocation,
  useNavigate,
  useSearchParams,
} from "../navigation";
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
  const redirecting = useRef(false);

  useEffect(() => {
    if (!auth.user || submitting) {
      redirecting.current = false;
      return;
    }
    if (redirecting.current) return;
    redirecting.current = true;
    void navigate("/", { replace: true });
  }, [auth.user, navigate, submitting]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSubmitting(true);
    setError("");
    try {
      if (mode === "login") await auth.login(email, password);
      else await auth.register(email, password);
      const fromState = (
        location.state as {
          from?: { pathname?: string; searchStr?: string; hash?: string };
        } | null
      )?.from;
      const search = fromState?.searchStr
        ? fromState.searchStr.startsWith("?")
          ? fromState.searchStr
          : `?${fromState.searchStr}`
        : "";
      const hash = fromState?.hash
        ? fromState.hash.startsWith("#")
          ? fromState.hash
          : `#${fromState.hash}`
        : "";
      const from = fromState?.pathname
        ? `${fromState.pathname}${search}${hash}`
        : "/";
      await navigate(from, { replace: true });
    } catch (reason) {
      setError(
        reason instanceof Error
          ? errorMessage(reason)
          : t("The request could not be completed"),
      );
    } finally {
      setSubmitting(false);
    }
  }

  const registering = mode === "register";
  return (
    <AuthFrame title={registering ? t("Create account") : t("Sign in")}>
      <form className="auth-form" onSubmit={(event) => void submit(event)}>
        {error && (
          <div className="alert" role="alert">
            {error}
          </div>
        )}
        <label>
          {t("Email")}{" "}
          <input
            type="email"
            autoComplete="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            required
          />
        </label>
        <label>
          {t("Password")}
          <input
            type="password"
            minLength={12}
            autoComplete={registering ? "new-password" : "current-password"}
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            required
          />
        </label>
        <button className="primary-button" disabled={submitting}>
          {registering ? <UserPlus size={17} /> : <LogIn size={17} />}
          {submitting
            ? "Working..."
            : registering
              ? t("Create account")
              : t("Sign in")}
        </button>
        {authFeatures.oauth &&
          !registering &&
          authFeatures.oauthProviders.map((provider) => (
            <button
              key={provider}
              type="button"
              className="secondary-button"
              onClick={() => authApi.startOAuth(provider)}
            >
              {" "}
              {t("Continue with")}{" "}
              {provider === "google"
                ? "Google"
                : provider === "github"
                  ? "GitHub"
                  : "SSO"}
            </button>
          ))}
        <div className="auth-links">
          {authFeatures.passwordReset && (
            <Link to="/forgot-password">{t("Forgot password?")}</Link>
          )}
          {authFeatures.registration && (
            <Link to={registering ? "/login" : "/register"}>
              {registering ? t("Sign in") : t("Create account")}
            </Link>
          )}
        </div>
      </form>
    </AuthFrame>
  );
}

export function ForgotPasswordPage() {
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState("");
  if (!authFeatures.passwordReset) return <Navigate to="/login" replace />;
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    try {
      await authApi.requestPasswordReset(email);
      setSent(true);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? errorMessage(reason)
          : t("The request could not be completed"),
      );
    }
  }
  return (
    <AuthFrame title={t("Reset password")}>
      {sent ? (
        <div className="auth-success">
          <Mail size={20} /> {t("Check your email")}
        </div>
      ) : (
        <form className="auth-form" onSubmit={(event) => void submit(event)}>
          {error && (
            <div className="alert" role="alert">
              {error}
            </div>
          )}
          <label>
            {t("Email")}{" "}
            <input
              type="email"
              autoComplete="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              required
            />
          </label>
          <button className="primary-button">
            <Mail size={17} /> {t("Send reset link")}
          </button>
          <div className="auth-links">
            <Link to="/login">{t("Back to sign in")}</Link>
          </div>
        </form>
      )}
    </AuthFrame>
  );
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
    try {
      await authApi.resetPassword(token, password);
      navigate("/login", { replace: true });
    } catch (reason) {
      setError(
        reason instanceof Error
          ? errorMessage(reason)
          : t("The request could not be completed"),
      );
    }
  }
  return (
    <AuthFrame title={t("Choose a password")}>
      <form className="auth-form" onSubmit={(event) => void submit(event)}>
        {error && (
          <div className="alert" role="alert">
            {error}
          </div>
        )}
        <label>
          {" "}
          {t("New password")}
          <input
            type="password"
            minLength={12}
            autoComplete="new-password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            required
          />
        </label>
        <button className="primary-button" disabled={!token}>
          <KeyRound size={17} /> {t("Update password")}
        </button>
      </form>
    </AuthFrame>
  );
}

export function VerifyEmailPage() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const [state, setState] = useState<"pending" | "success" | "error">(
    "pending",
  );
  const [message, setMessage] = useState("");
  const token = params.get("token") ?? "";
  const requestedToken = useRef<string | null>(null);
  useEffect(() => {
    if (!token) {
      setState("error");
      setMessage(t("This verification link is missing its token."));
      return;
    }
    if (requestedToken.current === token) return;
    requestedToken.current = token;
    let active = true;
    authApi
      .verifyEmail(token)
      .then(() => {
        if (active) {
          setState("success");
          setMessage(t("Your email address is verified."));
        }
      })
      .catch((reason) => {
        if (active) setState("error");
        if (active)
          setMessage(
            reason instanceof Error
              ? errorMessage(reason)
              : t("The verification link is invalid or expired"),
          );
      });
    return () => {
      active = false;
    };
  }, [token]);
  return (
    <AuthFrame
      title={
        state === "success"
          ? t("Email verified")
          : state === "error"
            ? t("Verification unavailable")
            : t("Verifying email")
      }
    >
      {state === "pending" ? (
        <div className="auth-loading" aria-label={t("Loading")} />
      ) : state === "success" ? (
        <>
          <div className="auth-success">
            <Mail size={20} /> {message}
          </div>
          <button
            type="button"
            className="primary-button"
            onClick={() => navigate("/", { replace: true })}
          >
            {" "}
            {t("Continue")}
          </button>
        </>
      ) : (
        <div className="alert" role="alert">
          {message}
        </div>
      )}
    </AuthFrame>
  );
}

function AuthFrame({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <main className="auth-page">
      <section className="auth-panel">
        <div className="auth-brand">__APP_TITLE__</div>
        <h1>{title}</h1>
        {children}
      </section>
    </main>
  );
}
