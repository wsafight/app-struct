import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { reportApi } from "../generated/client";
import { Link } from "../navigation";
import { errorMessage, triggerDownload } from "../resource";
import {
  ActionFeedback,
  EmptyState,
  LoadingState,
  formatDateTime,
  formatNumber,
  statusLabel,
  t,
  useActionFeedback,
} from "../ui";
import { appQueryKeys, workRefreshInterval } from "../query";

export function TaskCenterPage() {
  const client = useQueryClient();
  const [page, setPage] = useState(1);
  const action = useActionFeedback(errorMessage);
  const retryKeys = useRef(new Map<string, string>());
  const scope = appQueryKeys.reports.all();
  const query = useQuery({
    queryKey: appQueryKeys.reports.runs(page, 25),
    queryFn: ({ signal }) => reportApi.list(page, 25, { signal }),
    refetchInterval: (state) => workRefreshInterval(state.state.data?.data),
    refetchOnWindowFocus: true,
  });
  async function runAction(
    id: string,
    operation: () => Promise<unknown>,
    notice: string,
  ) {
    await action.run(
      id,
      async () => {
        await operation();
        await Promise.all([
          client.invalidateQueries({ queryKey: scope }),
          client.invalidateQueries({ queryKey: appQueryKeys.product.all }),
          client.invalidateQueries({ queryKey: ["billing"] }),
        ]);
      },
      notice,
    );
  }
  return (
    <main className="page">
      <h1>{t("Tasks and results")}</h1>
      <p>
        {t(
          "Follow your work, download completed results, or retry failed tasks.",
        )}
      </p>
      <Link to="/reports">{t("Create a report")}</Link>
      <ActionFeedback
        error={action.error || (query.error ? errorMessage(query.error) : "")}
        notice={action.notice}
        onRetry={query.error ? () => void query.refetch() : undefined}
      />
      {query.isPending && <LoadingState />}
      {query.data?.data.length === 0 && (
        <EmptyState
          title={t("No tasks yet")}
          description={t(
            "Create your first report to see progress and results here.",
          )}
        >
          <Link className="primary-button" to="/reports">
            {t("Create a report")}
          </Link>
        </EmptyState>
      )}
      {query.data?.data.map((run) => (
        <article
          className="panel result-card"
          key={run.id}
          aria-busy={action.isPending(run.id)}
        >
          <h2>{run.template}</h2>
          <p>
            {formatDateTime(run.created_at)} ·{" "}
            <span className={`report-status ${run.stage}`}>
              {statusLabel(run.stage)}
            </span>
          </p>
          <progress
            max={100}
            value={run.progress}
            aria-label={t("Task progress")}
          />
          <p>{formatNumber(run.progress)}%</p>
          {run.error_code && (
            <p>
              {t("Task failed. Retry it or share this code with support.")}{" "}
              <code>{run.error_code}</code>
            </p>
          )}
          {new Date(run.expires_at).getTime() <= Date.now() ? (
            <p>{t("Result retention expired")}</p>
          ) : (
            <>
              {run.stage === "succeeded" && (
                <button
                  type="button"
                  disabled={action.isPending(run.id)}
                  onClick={() =>
                    void action.run(
                      run.id,
                      async () =>
                        triggerDownload(
                          await reportApi.download(run.id),
                          `${run.template}-${run.id}.pdf`,
                        ),
                      t("Download started"),
                    )
                  }
                >
                  {t("Download PDF")}
                </button>
              )}
              {["queued", "rendering", "publishing"].includes(run.stage) && (
                <button
                  type="button"
                  disabled={action.isPending(run.id)}
                  onClick={() =>
                    void runAction(
                      run.id,
                      () => reportApi.cancel(run.id),
                      t("Task cancelled"),
                    )
                  }
                >
                  {t("Cancel task")}
                </button>
              )}
              {["failed", "cancelled"].includes(run.stage) && (
                <button
                  type="button"
                  disabled={action.isPending(run.id)}
                  onClick={() =>
                    void runAction(
                      run.id,
                      async () => {
                        const idempotencyKey =
                          retryKeys.current.get(run.id) ?? crypto.randomUUID();
                        retryKeys.current.set(run.id, idempotencyKey);
                        await reportApi.retry(run.id, idempotencyKey);
                      },
                      t("A new task was created"),
                    )
                  }
                >
                  {t("Retry as a new task")}
                </button>
              )}
            </>
          )}
          {action.isPending(run.id) && <p role="status">{t("Working...")}</p>}
        </article>
      ))}
      <button
        type="button"
        disabled={page === 1 || query.isFetching}
        onClick={() => setPage(page - 1)}
      >
        {t("Previous")}
      </button>
      <button
        type="button"
        disabled={
          !query.data ||
          page * query.data.meta.page_size >= query.data.meta.total ||
          query.isFetching
        }
        onClick={() => setPage(page + 1)}
      >
        {t("Next")}
      </button>
    </main>
  );
}
