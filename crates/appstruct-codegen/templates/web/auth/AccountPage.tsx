import { useState } from "react";
import { Link, useNavigate } from "../navigation";
import { workspacePath } from "../generated/registry";
import { errorMessage } from "../resource";
import { t } from "../ui";
import { useAuth } from "./Auth";
import { AccountSettings } from "./AccountSettings";

export function AccountPage() {
  const auth = useAuth();
  const navigate = useNavigate();
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  async function logout() {
    setPending(true);
    setError("");
    try {
      await auth.logout();
      await navigate("/login", { replace: true });
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }
  return (
    <main className="page account-page">
      <h1>{t("Account")}</h1>
      <section className="panel">
        <h2>{t("Email")}</h2>
        <p>{auth.user?.email}</p>
        <Link className="primary-button" to={workspacePath}>
          {t("Open workspace")}
        </Link>
        <button
          className="secondary-button"
          type="button"
          disabled={pending}
          onClick={() => void logout()}
        >
          {t("Sign out")}
        </button>
      </section>
      <AccountSettings />
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
    </main>
  );
}
