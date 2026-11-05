import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  currentTenant,
  taskApi,
  reportApi,
  productApi,
} from "../../generated/web/src/generated/client";
import { useAuth } from "../../generated/web/src/auth/Auth";
import {
  errorMessage,
  triggerDownload,
} from "../../generated/web/src/resource";
import { collaborationApi, type Approval } from "./collaboration-api";
import { appQueryKeys, resourceQueryKeys } from "../../generated/web/src/query";
import {
  ActionFeedback,
  EmptyState,
  LoadingState,
  formatDateTime,
  statusLabel,
  t,
} from "../../generated/web/src/ui";

export function ApprovalsPage() {
  const auth = useAuth();
  const client = useQueryClient();
  const [page, setPage] = useState(1);
  const [task, setTask] = useState("");
  const [report, setReport] = useState("");
  const [reviewer, setReviewer] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [pending, setPending] = useState(false);
  const submission = useRef<{ input: string; key: string } | null>(null);
  const query = useQuery({
    queryKey: ["approvals", currentTenant(), page],
    queryFn: ({ signal }) => collaborationApi.approvals(page, { signal }),
    refetchInterval: (query) =>
      query.state.data?.data.some((item) => item.state === "pending")
        ? 15_000
        : false,
    refetchOnWindowFocus: true,
  });
  const tasks = useQuery({
    queryKey: ["approval-tasks", currentTenant()],
    queryFn: ({ signal }) => taskApi.list({ page_size: 100 }, { signal }),
  });
  const reports = useQuery({
    queryKey: appQueryKeys.reports.runs(1, 100),
    queryFn: ({ signal }) => reportApi.list(1, 100, { signal }),
  });
  const team = useQuery({
    queryKey: appQueryKeys.product.team(),
    queryFn: ({ signal }) => productApi.team({ signal }),
  });
  async function submit() {
    setPending(true);
    setError("");
    setNotice("");
    try {
      const input = {
        task_id: task,
        report_run_id: report,
        reviewer_id: reviewer,
        note,
      };
      const material = JSON.stringify({ ...input, tenant: currentTenant() });
      if (submission.current?.input !== material)
        submission.current = { input: material, key: crypto.randomUUID() };
      await collaborationApi.submitApproval(input, submission.current.key);
      submission.current = null;
      setTask("");
      setReport("");
      setReviewer("");
      setNote("");
      setNotice(t("Review requested"));
      await client.invalidateQueries({ queryKey: ["approvals"] });
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }
  const queryError = query.error || tasks.error || reports.error || team.error;
  return (
    <main className="page">
      <h1>{t("Approvals")}</h1>
      <p>
        {t(
          "Ask another team member to review a task and its supporting report. Approval completes the unchanged task; rejection preserves it. Decisions remain in the history.",
        )}
      </p>
      <ActionFeedback
        error={error || (queryError ? errorMessage(queryError) : "")}
        notice={notice}
        onRetry={
          queryError
            ? () => {
                void query.refetch();
                void tasks.refetch();
                void reports.refetch();
                void team.refetch();
              }
            : undefined
        }
      />
      <form
        className="panel"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <h2>{t("Request a review")}</h2>
        <p>
          {t(
            "Selectors show up to 100 recent tasks/reports and 1000 team members.",
          )}
        </p>
        <label>
          {t("Task")}
          <select
            required
            value={task}
            disabled={pending}
            onChange={(event) => setTask(event.target.value)}
          >
            <option value="">{t("Choose a task")}</option>
            {tasks.data?.data
              .filter((item) => item.status !== "done")
              .map((item) => (
                <option key={item.id} value={item.id}>
                  {item.title}
                </option>
              ))}
          </select>
        </label>
        <label>
          {t("Completed report")}
          <select
            required
            value={report}
            disabled={pending}
            onChange={(event) => setReport(event.target.value)}
          >
            <option value="">{t("Choose your report")}</option>
            {reports.data?.data
              .filter(
                (item) =>
                  item.stage === "succeeded" &&
                  item.actor_id === auth.user?.id &&
                  new Date(item.expires_at).getTime() > Date.now(),
              )
              .map((item) => (
                <option key={item.id} value={item.id}>
                  {item.template} · {item.id}
                </option>
              ))}
          </select>
        </label>
        <label>
          {t("Reviewer")}
          <select
            required
            value={reviewer}
            disabled={pending}
            onChange={(event) => setReviewer(event.target.value)}
          >
            <option value="">{t("Choose another member")}</option>
            {team.data?.members
              .filter((item) => item.id !== auth.user?.id)
              .map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name || item.id}
                </option>
              ))}
          </select>
        </label>
        <label>
          {t("Note")}
          <textarea
            maxLength={2000}
            value={note}
            disabled={pending}
            onChange={(event) => setNote(event.target.value)}
          />
        </label>
        <button disabled={pending}>
          {t(pending ? "Submitting..." : "Submit for approval")}
        </button>
      </form>
      {query.isPending && <LoadingState label={t("Loading approvals")} />}
      {query.data?.data.length === 0 && (
        <EmptyState
          title={t("No approvals yet")}
          description={t(
            "Choose a task, a completed report and a reviewer to request your first review.",
          )}
        />
      )}
      {query.data?.data.map((item) => (
        <ApprovalCard
          key={item.id}
          approval={item}
          actor={auth.user?.id ?? ""}
        />
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
          !query.data || query.data.data.length < 25 || query.isFetching
        }
        onClick={() => setPage(page + 1)}
      >
        {t("Next")}
      </button>
    </main>
  );
}

function ApprovalCard({
  approval,
  actor,
}: {
  approval: Approval;
  actor: string;
}) {
  const client = useQueryClient();
  const [note, setNote] = useState("");
  const [showHistory, setShowHistory] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const taskScope = resourceQueryKeys.all("work::Task");
  const history = useQuery({
    queryKey: ["approval-history", approval.id],
    queryFn: ({ signal }) =>
      collaborationApi.approvalHistory(approval.id, { signal }),
    enabled: showHistory,
  });
  async function action(
    operation: () => Promise<unknown>,
    success = t("Approval updated"),
  ) {
    setPending(true);
    setError("");
    setNotice("");
    try {
      await operation();
      await client.invalidateQueries({ queryKey: ["approvals"] });
      await client.invalidateQueries({
        queryKey: ["approval-history", approval.id],
      });
      await client.invalidateQueries({ queryKey: ["planning"] });
      await client.invalidateQueries({ queryKey: ["approval-tasks"] });
      await client.invalidateQueries({ queryKey: taskScope });
      await client.invalidateQueries({ queryKey: appQueryKeys.product.all });
      setNotice(success);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }
  return (
    <article className="panel">
      <h2>{approval.task_snapshot.title}</h2>
      <p>
        {statusLabel(approval.state)} ·{" "}
        {t("Submitted {0}", { "0": formatDateTime(approval.created_at) })}
      </p>
      <p>{approval.note}</p>
      <details>
        <summary>{t("Submitted task snapshot")}</summary>
        <pre>{JSON.stringify(approval.task_snapshot, null, 2)}</pre>
      </details>
      <ActionFeedback
        error={error || (history.error ? errorMessage(history.error) : "")}
        notice={notice}
        onRetry={history.error ? () => void history.refetch() : undefined}
      />
      <button
        type="button"
        disabled={pending}
        onClick={() =>
          void action(
            async () =>
              triggerDownload(
                await collaborationApi.approvalReport(approval.id),
                "approval-report.pdf",
              ),
            t("Download started"),
          )
        }
      >
        {t("Download supporting report")}
      </button>
      {approval.state === "pending" && (
        <>
          <label>
            {t("Decision note")}
            <textarea
              maxLength={2000}
              value={note}
              onChange={(event) => setNote(event.target.value)}
            />
          </label>
          {approval.reviewer_id === actor && (
            <>
              <button
                type="button"
                disabled={pending}
                onClick={() =>
                  void action(() =>
                    collaborationApi.decide(approval.id, "approved", note),
                  )
                }
              >
                {t("Approve and complete task")}
              </button>
              <button
                type="button"
                disabled={pending}
                onClick={() =>
                  void action(() =>
                    collaborationApi.decide(approval.id, "rejected", note),
                  )
                }
              >
                {t("Reject")}
              </button>
            </>
          )}
          {approval.requested_by === actor && (
            <button
              type="button"
              disabled={pending}
              onClick={() =>
                void action(() =>
                  collaborationApi.decide(approval.id, "cancelled", note),
                )
              }
            >
              {t("Cancel request")}
            </button>
          )}
        </>
      )}
      <button
        type="button"
        aria-expanded={showHistory}
        onClick={() => setShowHistory(!showHistory)}
      >
        {t("Decision history")}
      </button>
      {showHistory && (
        <ol>
          {history.data?.data.map((item) => (
            <li key={item.id}>
              {statusLabel(item.decision)} · {item.actor_id} ·{" "}
              {formatDateTime(item.created_at)}
              <p>{item.note}</p>
            </li>
          ))}
        </ol>
      )}
    </article>
  );
}
