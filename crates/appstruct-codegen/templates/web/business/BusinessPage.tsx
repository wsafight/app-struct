import { useQuery } from "@tanstack/react-query";
import { useAuth } from "../auth/Auth";
import { businessApi } from "../generated/client";
import { Link, Navigate } from "../navigation";
import { errorMessage } from "../resource";
import { formatDateTime, formatNumber, t } from "../ui";

const labels: Record<string, string> = {
  registered_accounts: "Registered accounts",
  organizations: "Organizations",
  successful_reports: "Successful reports",
  activated_accounts: "Activated accounts",
  paying_organizations: "Paying organizations",
  visits: "Visits",
  revenue: "Revenue",
};

export function BusinessPage() {
  const auth = useAuth();
  const admin = auth.user?.roles.includes("admin") ?? false;
  const query = useQuery({
    queryKey: ["business-overview"],
    queryFn: ({ signal }) => businessApi.overview({ signal }),
    enabled: admin,
    refetchInterval: 60_000,
  });
  if (!admin) return <Navigate to="/account" replace />;
  return (
    <main className="page">
      <Link to="/admin">{t("Administration")}</Link>
      <div className="page-heading">
        <div>
          <h1>{t("Business overview")}</h1>
          <p>
            {query.data
              ? t("Updated {0}", {
                  "0": formatDateTime(query.data.generated_at),
                })
              : t("Loading...")}
          </p>
        </div>
        <button
          type="button"
          className="secondary-button"
          disabled={query.isFetching}
          onClick={() => void query.refetch()}
        >
          {t("Refresh")}
        </button>
      </div>
      {query.error && (
        <div className="alert" role="alert">
          {errorMessage(query.error)}
        </div>
      )}
      <section className="business-metrics">
        {query.data?.metrics.map((metric) => (
          <article className="panel" key={metric.key}>
            <h2>{t(labels[metric.key] ?? metric.key)}</h2>
            <p className="business-value">
              {metric.value === null ? "—" : formatNumber(metric.value)}
            </p>
            <p>{t(metric.definition)}</p>
            <small>
              {t("Source")}: {t(metric.source)} · {t(metric.window)}
            </small>
            {metric.unavailable_reason && (
              <p role="status">{t(metric.unavailable_reason)}</p>
            )}
          </article>
        ))}
      </section>
    </main>
  );
}
