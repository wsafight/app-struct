import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  currentTenant,
  taskApi,
  projectApi,
  reportApi,
  type Task,
} from "../../generated/web/src/generated/client";
import { appQueryKeys, resourceQueryKeys } from "../../generated/web/src/query";
import { Link } from "../../generated/web/src/navigation";
import { errorMessage } from "../../generated/web/src/resource";
import {
  ActionFeedback,
  EmptyState,
  LoadingState,
  formatDate,
  formatNumber,
  statusLabel,
  t,
  useActionFeedback,
} from "../../generated/web/src/ui";
import "./product.css";

const stages = ["todo", "in_progress", "done"] as const;
type Stage = (typeof stages)[number];

export function PlanningPage() {
  const client = useQueryClient();
  const tenant = currentTenant();
  const [view, setView] = useState<"board" | "calendar">("board");
  const [presentation, setPresentation] = useState<"month" | "agenda">(() =>
    window.matchMedia("(max-width: 700px)").matches ? "agenda" : "month",
  );
  const [month, setMonth] = useState(() => {
    const now = new Date();
    return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
  });
  const [page, setPage] = useState(1);
  const action = useActionFeedback(errorMessage);
  const calendar = view === "calendar";
  const [year, monthIndex] = month.split("-").map(Number);
  const days = new Date(Date.UTC(year, monthIndex, 0)).getUTCDate();
  const offset = new Date(Date.UTC(year, monthIndex - 1, 1)).getUTCDay();
  const query = useQuery({
    queryKey: ["planning", tenant, view, month, page],
    queryFn: ({ signal }) =>
      taskApi.list(
        {
          page,
          page_size: 100,
          sort: "-created_at",
          ...(calendar
            ? {
                range_filters: {
                  due_date: { gte: `${month}-01`, lte: `${month}-${days}` },
                },
              }
            : {}),
        },
        { signal },
      ),
  });
  const projects = useQuery({
    queryKey: ["planning-project-count", tenant],
    queryFn: ({ signal }) => projectApi.list({ page_size: 1 }, { signal }),
  });
  const reports = useQuery({
    queryKey: appQueryKeys.reports.runs(1, 1),
    queryFn: ({ signal }) => reportApi.list(1, 1, { signal }),
  });
  const tasks = (query.data?.data ?? []).filter(
    (task): task is Task =>
      typeof task.id === "string" &&
      typeof task.title === "string" &&
      stages.some((stage) => stage === task.status),
  );
  async function move(id: string, status: Stage) {
    await action.run(
      id,
      async () => {
        await taskApi.get(id);
        await taskApi.update(id, { status });
        await Promise.all([
          client.invalidateQueries({ queryKey: ["planning", tenant] }),
          client.invalidateQueries({
            queryKey: resourceQueryKeys.all("work::Task"),
          }),
        ]);
      },
      t("Task status updated"),
    );
  }
  function card(task: Task) {
    return (
      <article
        className="panel planning-card"
        key={task.id}
        aria-busy={action.isPending(task.id)}
      >
        <Link to={`/tasks/${task.id}`}>{task.title}</Link>
        <p>{task.due_date ? formatDate(task.due_date) : t("No due date")}</p>
        <label>
          {t("Status")}
          <select
            disabled={action.isPending(task.id)}
            value={task.status}
            onChange={(event) =>
              void move(task.id, event.target.value as Stage)
            }
          >
            {stages.map((stage) => (
              <option key={stage} value={stage}>
                {statusLabel(stage)}
              </option>
            ))}
          </select>
        </label>
        {action.isPending(task.id) && (
          <span role="status">{t("Saving...")}</span>
        )}
      </article>
    );
  }
  function chooseView(next: "board" | "calendar") {
    setView(next);
    setPage(1);
  }
  return (
    <main className="page planning-page">
      <h1>{t("Team planning")}</h1>
      <section className="product-grid planning-summary">
        <article className="panel">
          <span>{t("Team projects")}</span>
          <strong>
            {projects.data ? formatNumber(projects.data.meta.total) : "—"}
          </strong>
        </article>
        <article className="panel">
          <span>{t("Tasks in this view")}</span>
          <strong>
            {query.data ? formatNumber(query.data.meta.total) : "—"}
          </strong>
        </article>
        <article className="panel">
          <span>{t("Reports visible to you")}</span>
          <strong>
            {reports.data ? formatNumber(reports.data.meta.total) : "—"}
          </strong>
        </article>
      </section>
      <div
        className="product-actions view-switcher"
        aria-label={t("Planning view")}
      >
        <button
          type="button"
          aria-pressed={!calendar}
          onClick={() => chooseView("board")}
        >
          {t("Board")}
        </button>
        <button
          type="button"
          aria-pressed={calendar}
          onClick={() => chooseView("calendar")}
        >
          {t("Calendar")}
        </button>
        <Link className="primary-button" to="/tasks/new">
          {t("Create task")}
        </Link>
        <Link to="/approvals">{t("Review approvals")}</Link>
      </div>
      <ActionFeedback
        error={action.error || (query.error ? errorMessage(query.error) : "")}
        notice={action.notice}
        onRetry={query.error ? () => void query.refetch() : undefined}
      />
      {(projects.error || reports.error) && (
        <ActionFeedback
          error={t("Some workspace counts are unavailable")}
          onRetry={() => {
            void projects.refetch();
            void reports.refetch();
          }}
        />
      )}
      {calendar && (
        <div className="calendar-controls">
          <label>
            {t("Month")}
            <input
              type="month"
              required
              value={month}
              onChange={(event) => {
                if (/^\d{4}-(0[1-9]|1[0-2])$/.test(event.target.value)) {
                  setMonth(event.target.value);
                  setPage(1);
                }
              }}
            />
          </label>
          <div
            className="view-switcher"
            aria-label={t("Calendar presentation")}
          >
            <button
              type="button"
              aria-pressed={presentation === "agenda"}
              onClick={() => setPresentation("agenda")}
            >
              {t("Agenda")}
            </button>
            <button
              type="button"
              aria-pressed={presentation === "month"}
              onClick={() => setPresentation("month")}
            >
              {t("Month grid")}
            </button>
          </div>
        </div>
      )}
      {query.isPending ? (
        <LoadingState label={t("Loading tasks")} />
      ) : query.data && tasks.length === 0 ? (
        <EmptyState
          title={t("No tasks in this view")}
          description={t(
            "Create a task or choose another month to get started.",
          )}
        >
          <Link to="/tasks/new">{t("Create task")}</Link>
        </EmptyState>
      ) : calendar ? (
        presentation === "agenda" ? (
          <div className="planning-agenda">
            {Array.from(
              { length: days },
              (_, index) => `${month}-${String(index + 1).padStart(2, "0")}`,
            )
              .filter((date) => tasks.some((task) => task.due_date === date))
              .map((date) => (
                <section key={date} aria-label={formatDate(date)}>
                  <h2>{formatDate(date)}</h2>
                  {tasks.filter((task) => task.due_date === date).map(card)}
                </section>
              ))}
          </div>
        ) : (
          <div
            className="calendar-scroll"
            role="region"
            aria-label={t("Task calendar")}
            tabIndex={0}
          >
            <div className="planning-calendar">
              {["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].map((day) => (
                <strong key={day}>{t(day)}</strong>
              ))}
              {Array.from({ length: offset }, (_, index) => (
                <div aria-hidden key={`blank-${index}`} />
              ))}
              {Array.from({ length: days }, (_, index) => {
                const date = `${month}-${String(index + 1).padStart(2, "0")}`;
                return (
                  <section
                    className="calendar-day"
                    key={date}
                    aria-label={date}
                  >
                    <h2>{index + 1}</h2>
                    {tasks.filter((task) => task.due_date === date).map(card)}
                  </section>
                );
              })}
            </div>
          </div>
        )
      ) : (
        <div className="planning-board">
          {stages.map((stage) => (
            <section key={stage} aria-label={statusLabel(stage)}>
              <h2>
                {statusLabel(stage)}{" "}
                <span className="count-badge">
                  {formatNumber(
                    tasks.filter((task) => task.status === stage).length,
                  )}
                </span>
              </h2>
              {tasks.filter((task) => task.status === stage).map(card)}
              {!tasks.some((task) => task.status === stage) && (
                <p className="muted">{t("No tasks on this page")}</p>
              )}
            </section>
          ))}
        </div>
      )}
      {query.data && (
        <nav className="pagination" aria-label={t("Task pages")}>
          <p>
            {t("Page {0} · up to {1} tasks per page", {
              "0": formatNumber(page),
              "1": formatNumber(100),
            })}
          </p>
          <button
            type="button"
            disabled={page === 1 || query.isFetching}
            onClick={() => setPage(page - 1)}
          >
            {t("Previous")}
          </button>
          <button
            type="button"
            disabled={page * 100 >= query.data.meta.total || query.isFetching}
            onClick={() => setPage(page + 1)}
          >
            {t("Next")}
          </button>
        </nav>
      )}
    </main>
  );
}
