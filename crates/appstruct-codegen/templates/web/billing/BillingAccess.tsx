import type { ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { billingApi } from "../generated/client";
import { Link } from "../navigation";
import { errorMessage } from "../resource";
import { t } from "../ui";

export function BillingAccess({
  entitlement,
  children,
}: {
  entitlement: string;
  children: ReactNode;
}) {
  const query = useQuery({
    queryKey: ["billing"],
    queryFn: ({ signal }) => billingApi.overview({ signal }),
    refetchInterval: 5_000,
  });
  if (query.isPending) return <p role="status">{t("Loading...")}</p>;
  if (query.error)
    return (
      <div className="alert" role="alert">
        {errorMessage(query.error)}{" "}
        <button type="button" onClick={() => void query.refetch()}>
          {t("Retry")}
        </button>
      </div>
    );
  const allowance = query.data.usage.find(
    (item) => item.entitlement === entitlement,
  );
  const allowed =
    query.data.entitlements.includes(entitlement) ||
    (allowance?.remaining ?? 0) > 0;
  return (
    <section>
      {allowance && (
        <p role="status">
          {allowance.paid
            ? t("Included in your plan")
            : t("{0} of {1} free uses remaining", {
                "0": allowance.remaining,
                "1": allowance.free_limit,
              })}
        </p>
      )}
      {allowed ? (
        children
      ) : (
        <div className="panel">
          <h2>{t("Upgrade to continue")}</h2>
          <p>
            {t("The free allowance is exhausted. Choose a plan to continue.")}
          </p>
          <Link className="primary-button" to="/billing">
            {t("View plans")}
          </Link>
        </div>
      )}
    </section>
  );
}
