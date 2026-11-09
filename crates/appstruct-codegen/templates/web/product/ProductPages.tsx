import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { productApi, productFeatures } from "../generated/client";
import { Link } from "../navigation";
import { errorMessage } from "../resource";
import {
  ActionFeedback,
  EmptyState,
  LoadingState,
  formatDateTime,
  formatMinorMoney,
  formatNumber,
  t,
  useActionFeedback,
} from "../ui";
import { appQueryKeys } from "../query";
import { useAuth } from "../auth/Auth";

function useProductAction(refreshTeam = false) {
  const client = useQueryClient();
  const action = useActionFeedback(errorMessage);
  async function run(
    key: string,
    operation: () => Promise<unknown>,
    notice = t("Changes saved"),
  ) {
    return action.run(
      key,
      async () => {
        await operation();
        await Promise.all([
          client.invalidateQueries({ queryKey: appQueryKeys.product.all }),
          ...(refreshTeam
            ? [
                client.invalidateQueries({
                  queryKey: appQueryKeys.tenant.organizations,
                }),
              ]
            : []),
        ]);
      },
      notice,
    );
  }
  return { ...action, run };
}

export function NotificationsPage() {
  const [page, setPage] = useState(1);
  const query = useQuery({
    queryKey: appQueryKeys.product.notifications(page),
    queryFn: ({ signal }) => productApi.notifications(page, { signal }),
    refetchInterval: 30_000,
  });
  const action = useProductAction();
  return (
    <main className="page">
      <div className="page-heading">
        <div>
          <h1>{t("Notifications")}</h1>
          <p>
            {query.data ? formatNumber(query.data.unread) : "—"} {t("unread")}
          </p>
        </div>
        <button
          type="button"
          disabled={action.isPending("all") || !query.data?.unread}
          onClick={() =>
            void action.run(
              "all",
              productApi.readAllNotifications,
              t("All notifications marked as read"),
            )
          }
        >
          {t(action.isPending("all") ? "Working..." : "Mark all as read")}
        </button>
      </div>
      <ActionFeedback
        error={action.error || (query.error ? errorMessage(query.error) : "")}
        notice={action.notice}
        onRetry={query.error ? () => void query.refetch() : undefined}
      />
      {query.isPending && <LoadingState />}
      {query.data?.data.length === 0 && (
        <EmptyState
          title={t("No notifications yet")}
          description={t("Report results and team updates will appear here.")}
        />
      )}
      {query.data?.data.map((item) => (
        <article
          className={`panel notification-card ${item.read_at ? "is-read" : "is-unread"}`}
          key={item.id}
          aria-busy={action.isPending(item.id)}
        >
          <h2>
            {!item.read_at && (
              <span
                className="unread-dot"
                aria-label={t("Unread notification")}
              />
            )}
            {item.title}
          </h2>
          <p>{item.body}</p>
          <p className="muted">{formatDateTime(item.created_at)}</p>
          <div className="row-actions">
            <Link to={item.target}>{t("Open")}</Link>
            {!item.read_at && (
              <button
                type="button"
                disabled={action.isPending(item.id) || action.isPending("all")}
                onClick={() =>
                  void action.run(
                    item.id,
                    () => productApi.readNotification(item.id),
                    t("Notification marked as read"),
                  )
                }
              >
                {t(action.isPending(item.id) ? "Working..." : "Mark as read")}
              </button>
            )}
          </div>
        </article>
      ))}
      {query.data && query.data.total > 25 && (
        <nav className="pagination" aria-label={t("Notification pages")}>
          <button
            type="button"
            disabled={page === 1 || query.isFetching}
            onClick={() => setPage(page - 1)}
          >
            {t("Previous")}
          </button>
          <span>{t("Page {0}", { "0": formatNumber(page) })}</span>
          <button
            type="button"
            disabled={page * 25 >= query.data.total || query.isFetching}
            onClick={() => setPage(page + 1)}
          >
            {t("Next")}
          </button>
        </nav>
      )}
      <Link to="/account">{t("Notification preferences")}</Link>
    </main>
  );
}

export function TeamPage() {
  const auth = useAuth();
  const query = useQuery({
    queryKey: appQueryKeys.product.team(),
    queryFn: ({ signal }) => productApi.team({ signal }),
  });
  const [name, setName] = useState("");
  const [target, setTarget] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const action = useProductAction(true);
  const team = query.data;
  const member = team?.members.find((item) => item.id === target);
  return (
    <main className="page">
      <h1>{t("Team settings")}</h1>
      <ActionFeedback
        error={action.error || (query.error ? errorMessage(query.error) : "")}
        notice={action.notice}
        onRetry={query.error ? () => void query.refetch() : undefined}
      />
      {query.isPending && <LoadingState />}
      {team && (
        <>
          <section className="panel">
            <h2>{team.name}</h2>
            <p>
              {formatNumber(team.seats_used)} /{" "}
              {team.seat_limit ? formatNumber(team.seat_limit) : "∞"}{" "}
              {t("seats")}
            </p>
            <Link to="/organization">{t("Manage invitations")}</Link>
            {team.is_owner && (
              <form
                className="settings-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  void action.run(
                    "rename",
                    async () => {
                      await productApi.renameTeam(name.trim());
                      setName("");
                    },
                    t("Team name updated"),
                  );
                }}
              >
                <label>
                  {t("Team name")}
                  <input
                    value={name}
                    placeholder={team.name}
                    maxLength={120}
                    required
                    disabled={action.isPending("rename")}
                    onChange={(event) => setName(event.target.value)}
                  />
                </label>
                <button disabled={action.isPending("rename") || !name.trim()}>
                  {t(action.isPending("rename") ? "Saving..." : "Save")}
                </button>
              </form>
            )}
          </section>
          <section className="panel">
            <h2>{t("Team members")}</h2>
            <ul className="member-list">
              {team.members.map((item) => (
                <li key={item.id}>
                  <div>
                    <strong>{item.name || t("Member")}</strong>
                    <p>
                      <code>{item.id}</code> ·{" "}
                      {t(item.role === "owner" ? "Owner" : "Member")}
                    </p>
                  </div>
                  {item.role !== "owner" &&
                    (team.is_owner || item.id === auth.user?.id) && (
                      <button
                        type="button"
                        disabled={action.isPending("member")}
                        onClick={() => {
                          setTarget(item.id);
                          setConfirmation("");
                        }}
                      >
                        {t("Manage member")}
                      </button>
                    )}
                </li>
              ))}
            </ul>
            {team.members_truncated && (
              <p>{t("Only the first 1000 members are shown.")}</p>
            )}
          </section>
          {member && (
            <section
              className="panel danger-zone"
              aria-labelledby="member-change-title"
            >
              <h2 id="member-change-title">{t("Confirm member change")}</h2>
              <p>
                <strong>{member.name || t("Member")}</strong> ·{" "}
                <code>{member.id}</code>
              </p>
              <p>
                {team.is_owner
                  ? t(
                      "Transferring ownership gives this member control of the team. Removing a member revokes their team access.",
                    )
                  : t(
                      "Leaving this team revokes your access to its shared work.",
                    )}
              </p>
              <label>
                {t("Type the team name to confirm")}
                <input
                  value={confirmation}
                  disabled={action.isPending("member")}
                  onChange={(event) => setConfirmation(event.target.value)}
                />
              </label>
              <div className="row-actions">
                {team.is_owner && (
                  <button
                    type="button"
                    disabled={
                      action.isPending("member") || confirmation !== team.name
                    }
                    onClick={() =>
                      void action.run(
                        "member",
                        async () => {
                          await productApi.transferTeam(target);
                          setTarget("");
                          setConfirmation("");
                        },
                        t("Ownership transferred"),
                      )
                    }
                  >
                    {t("Transfer ownership")}
                  </button>
                )}
                <button
                  type="button"
                  disabled={
                    action.isPending("member") || confirmation !== team.name
                  }
                  onClick={() =>
                    void action.run(
                      "member",
                      async () => {
                        await productApi.removeMember(target);
                        setTarget("");
                        setConfirmation("");
                      },
                      t("Member removed"),
                    )
                  }
                >
                  {t("Remove member")}
                </button>
                <button
                  type="button"
                  disabled={action.isPending("member")}
                  onClick={() => {
                    setTarget("");
                    setConfirmation("");
                  }}
                >
                  {t("Cancel")}
                </button>
              </div>
            </section>
          )}
        </>
      )}
    </main>
  );
}

export function CreditsPage() {
  const [page, setPage] = useState(1);
  const key = useRef<string | null>(null);
  const query = useQuery({
    queryKey: appQueryKeys.product.credits(page),
    queryFn: ({ signal }) => productApi.credits(page, { signal }),
    refetchInterval: 15_000,
  });
  const team = useQuery({
    queryKey: appQueryKeys.product.team(),
    queryFn: ({ signal }) => productApi.team({ signal }),
  });
  const action = useProductAction();
  const credits = query.data;
  return (
    <main className="page">
      <h1>{t("Credits and monthly allowance")}</h1>
      <ActionFeedback
        error={action.error || (query.error ? errorMessage(query.error) : "")}
        notice={action.notice}
        onRetry={query.error ? () => void query.refetch() : undefined}
      />
      {query.isPending && <LoadingState />}
      {team.error && (
        <ActionFeedback
          error={errorMessage(team.error)}
          onRetry={() => void team.refetch()}
        />
      )}
      {credits && (
        <>
          <section className="panel">
            <h2>
              {formatNumber(credits.available)} {t("credits available")}
            </h2>
            <p>
              {t("Monthly remaining")}:{" "}
              {formatNumber(credits.monthly_remaining)} /{" "}
              {formatNumber(credits.monthly_allowance)}
            </p>
            <p>
              {t("Purchased balance")}: {formatNumber(credits.paid_balance)}
            </p>
            <p>
              {t("Monthly reset")}: {formatDateTime(credits.resets_at)}
            </p>
            <p>
              {t(
                "Monthly credits expire at reset. Purchased credits do not expire. Monthly credits are used first.",
              )}
            </p>
            {credits.paid_balance < 0 && (
              <p role="alert">
                {t(
                  "A payment refund left a credit debt. Add credits before continuing.",
                )}
              </p>
            )}
            {productFeatures.billing && team.data?.is_owner && (
              <button
                type="button"
                disabled={action.isPending("checkout")}
                onClick={() =>
                  void action.run(
                    "checkout",
                    async () => {
                      key.current ??= crypto.randomUUID();
                      const result = await productApi.checkoutCredits(
                        key.current,
                      );
                      window.location.assign(result.url);
                    },
                    "",
                  )
                }
              >
                {t(
                  action.isPending("checkout")
                    ? "Opening checkout..."
                    : "Buy a credit pack",
                )}
              </button>
            )}
          </section>
          <h2>{t("Credit ledger")}</h2>
          {credits.data.length === 0 ? (
            <EmptyState title={t("No credit entries on this page")} />
          ) : (
            <section className="table-frame">
              <table>
                <thead>
                  <tr>
                    <th>{t("Date")}</th>
                    <th>{t("Credits")}</th>
                    <th>{t("Reason")}</th>
                  </tr>
                </thead>
                <tbody>
                  {credits.data.map((entry) => (
                    <tr key={entry.id}>
                      <td>{formatDateTime(entry.created_at)}</td>
                      <td>{formatNumber(entry.amount)}</td>
                      <td>{t(entry.reason)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}
          <nav className="pagination" aria-label={t("Credit ledger pages")}>
            <button
              type="button"
              disabled={page === 1 || query.isFetching}
              onClick={() => setPage(page - 1)}
            >
              {t("Previous")}
            </button>
            <span>{t("Page {0}", { "0": formatNumber(page) })}</span>
            <button
              type="button"
              disabled={
                credits.data.length < credits.page_size || query.isFetching
              }
              onClick={() => setPage(page + 1)}
            >
              {t("Next")}
            </button>
          </nav>
        </>
      )}
    </main>
  );
}

export function InsightsPage() {
  const auth = useAuth();
  const allowed = auth.user?.roles.includes("admin") ?? false;
  const query = useQuery({
    queryKey: ["product", "insights"],
    queryFn: ({ signal }) => productApi.insights({ signal }),
    enabled: allowed,
    refetchInterval: 60_000,
  });
  if (!allowed)
    return (
      <main className="page">
        <p role="alert">{t("Administrator access required")}</p>
      </main>
    );
  const data = query.data;
  return (
    <main className="page">
      <h1>{t("Business funnel and retention")}</h1>
      <ActionFeedback
        error={query.error ? errorMessage(query.error) : ""}
        onRetry={() => void query.refetch()}
      />
      {query.isPending && <LoadingState />}
      {data && (
        <>
          <p>
            {t("Last 30 days")} · {formatDateTime(data.generated_at)}
          </p>
          <section className="product-grid">
            {[
              ["Visitors", data.visitors],
              ["Registrations", data.signups],
              ["Activated users", data.activated],
            ].map(([label, value]) => (
              <article className="panel" key={label}>
                <h2>{t(String(label))}</h2>
                <strong>{formatNumber(Number(value))}</strong>
              </article>
            ))}
          </section>
          <p>{t(data.definitions.funnel)}</p>
          <h2>{t("Visitor sources")}</h2>
          {data.sources.length === 0 ? (
            <EmptyState title={t("No consented visits in this window")} />
          ) : (
            <section className="table-frame">
              <table>
                <thead>
                  <tr>
                    <th>{t("Source")}</th>
                    <th>{t("Medium")}</th>
                    <th>{t("Visitors")}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.sources.map((row) => (
                    <tr key={`${row.source}:${row.medium ?? ""}`}>
                      <td>{row.source}</td>
                      <td>{row.medium ?? "—"}</td>
                      <td>{formatNumber(row.visitors)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}
          <section className="panel">
            <h2>{t("Day-7 retention")}</h2>
            <p>
              {formatNumber(data.retention_d7.returned)} /{" "}
              {formatNumber(data.retention_d7.eligible)}
            </p>
            <p>{t(data.definitions.retention_d7)}</p>
          </section>
          <h2>{t("Payment receipts and refunds")}</h2>
          {!data.revenue_available ? (
            <p>{t("Payment event source is not configured")}</p>
          ) : data.revenue.length === 0 ? (
            <EmptyState
              title={t("No captured payment events in this window")}
            />
          ) : (
            <section className="table-frame">
              <table>
                <thead>
                  <tr>
                    <th>{t("Currency")}</th>
                    <th>{t("Captured")}</th>
                    <th>{t("Refunded")}</th>
                    <th>{t("Net")}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.revenue.map((row) => (
                    <tr key={row.currency}>
                      <td>{row.currency.toUpperCase()}</td>
                      <td>
                        {formatMinorMoney(row.captured_minor, row.currency)}
                      </td>
                      <td>
                        {formatMinorMoney(row.refunded_minor, row.currency)}
                      </td>
                      <td>{formatMinorMoney(row.net_minor, row.currency)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}
          <p>{t(data.definitions.revenue)}</p>
          <details>
            <summary>{t("Metric definitions")}</summary>
            <dl>
              {Object.entries(data.definitions).map(([name, definition]) => (
                <div key={name}>
                  <dt>
                    {t(
                      (
                        {
                          visitors: "Visitors",
                          signups: "Registrations",
                          activated: "Activated users",
                          retention_d7: "Day-7 retention",
                          revenue: "Payment receipts and refunds",
                          sources: "Visitor sources",
                          funnel: "Funnel",
                        } as Record<string, string>
                      )[name] ?? name,
                    )}
                  </dt>
                  <dd>{t(definition)}</dd>
                </div>
              ))}
            </dl>
          </details>
        </>
      )}
    </main>
  );
}
