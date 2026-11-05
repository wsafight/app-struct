import { Link } from "../../generated/web/src/navigation";
import { workspacePath } from "../../generated/web/src/generated/registry";
import "./product.css";
import { VisitConsent } from "./VisitConsent";
import { t } from "../../generated/web/src/ui";

const start = `/register?next=${encodeURIComponent("/studio")}`;

export function LandingPage() {
  return (
    <main className="product-page">
      <title>{t("Team Reports — private reports for your team")}</title>
      <meta
        name="description"
        content={t(
          "Upload team data, follow report progress, review work and download finished documents in a private workspace.",
        )}
      />
      <section className="product-hero">
        <p className="product-eyebrow">TEAM REPORTS</p>
        <h1>{t("Turn your team's data into a report worth sharing.")}</h1>
        <p>
          {t(
            "Keep your inputs, report jobs and finished documents together in one private workspace.",
          )}
        </p>
        <div className="product-actions">
          <Link className="primary-button" to={start}>
            {t("Create your workspace")}
          </Link>
          <Link className="secondary-button" to="/pricing">
            {t("Explore plans")}
          </Link>
        </div>
      </section>
      <section className="product-grid" aria-label={t("How it works")}>
        <article className="panel">
          <span className="product-step">01</span>
          <h2>{t("Bring your data")}</h2>
          <p>
            {t(
              "Upload the files your team uses and keep them with the work they support.",
            )}
          </p>
        </article>
        <article className="panel">
          <span className="product-step">02</span>
          <h2>{t("Build a report")}</h2>
          <p>
            {t(
              "Submit a report, follow its progress and download the finished result.",
            )}
          </p>
        </article>
        <article className="panel">
          <span className="product-step">03</span>
          <h2>{t("Grow with your team")}</h2>
          <p>
            {t(
              "Start with the free allowance and upgrade when your team needs more.",
            )}
          </p>
        </article>
      </section>
      <VisitConsent page="/" />
    </main>
  );
}

export function PricingPage() {
  return (
    <main className="product-page">
      <title>{t("Plans and pricing — Team Reports")}</title>
      <meta
        name="description"
        content={t(
          "Try three successful reports per organization, then upgrade your team with subscriptions and optional AI credits.",
        )}
      />
      <header className="product-intro">
        <p className="product-eyebrow">{t("PLANS")}</p>
        <h1>{t("Start small. Make room for more.")}</h1>
        <p>
          {t("Each organization has its own files, reports and subscription.")}
        </p>
      </header>
      <section className="product-grid product-plans" aria-label={t("Plans")}>
        <article className="panel">
          <h2>{t("Free")}</h2>
          <p className="product-price">{t("Try the workflow")}</p>
          <ul>
            <li>{t("Private team workspaces")}</li>
            <li>{t("Three successful reports per organization")}</li>
            <li>{t("Upload and download your files")}</li>
          </ul>
          <Link className="secondary-button" to={start}>
            {t("Get started")}
          </Link>
        </article>
        <article className="panel product-featured">
          <p className="product-eyebrow">{t("FOR GROWING TEAMS")}</p>
          <h2>Pro</h2>
          <p className="product-price">{t("Subscription")}</p>
          <ul>
            <li>{t("Continue beyond the free report allowance")}</li>
            <li>{t("Team access and report history")}</li>
            <li>{t("Manage your subscription through the billing portal")}</li>
          </ul>
          <Link className="primary-button" to="/billing">
            {t("Choose Pro")}
          </Link>
          <p>
            {t(
              "Your organization owner can review the configured price at checkout.",
            )}
          </p>
        </article>
      </section>
      <p className="product-note">
        {t("Already have an account?")}{" "}
        <Link to={`/login?next=${encodeURIComponent(workspacePath)}`}>
          {t("Open your workspace")}
        </Link>{" "}
        {t("or")} <Link to="/account">{t("view your account")}</Link>.
      </p>
      <VisitConsent page="/pricing" />
    </main>
  );
}
