import { useEffect, useState } from "react";
import { productApi } from "../../generated/web/src/generated/client";
import { t } from "../../generated/web/src/ui";

function consented() {
  try {
    return localStorage.getItem("appstruct_analytics_consent") === "yes";
  } catch {
    return false;
  }
}

export function VisitConsent({ page }: { page: "/" | "/pricing" }) {
  const [enabled, setEnabled] = useState(consented);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!enabled) return;
    let active = true;
    try {
      let visitor = localStorage.getItem("appstruct_analytics_visitor");
      if (!visitor) {
        visitor = crypto.randomUUID();
        localStorage.setItem("appstruct_analytics_visitor", visitor);
      }
      void productApi.recordVisit(visitor, page).catch(() => {
        if (active) setError(t("Visit measurement is unavailable."));
      });
    } catch {
      setError(
        t("Browser storage is unavailable; measurement stays disabled."),
      );
    }
    return () => {
      active = false;
    };
  }, [enabled, page]);
  function toggle() {
    try {
      localStorage.setItem(
        "appstruct_analytics_consent",
        enabled ? "no" : "yes",
      );
      if (enabled) localStorage.removeItem("appstruct_analytics_visitor");
      setEnabled(!enabled);
      setError("");
    } catch {
      setError(t("Your preference could not be saved."));
    }
  }
  return (
    <aside
      className="product-note"
      aria-label={t("Visit measurement preference")}
    >
      <p>
        {t(
          "Optional first-party visit measurement uses a random browser ID, page and UTC date. No third-party tracking script is loaded.",
        )}
      </p>
      <button type="button" onClick={toggle}>
        {t(enabled ? "Disable visit measurement" : "Allow visit measurement")}
      </button>
      {error && <p role="status">{error}</p>}
    </aside>
  );
}
