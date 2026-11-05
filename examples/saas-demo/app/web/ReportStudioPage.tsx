import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { fileApi, reportApi } from "../../generated/web/src/generated/client";
import { FileUpload } from "../../generated/web/src/file/FilesPage";
import { BillingAccess } from "../../generated/web/src/billing/BillingAccess";
import { Link } from "../../generated/web/src/navigation";
import {
  errorMessage,
  triggerDownload,
} from "../../generated/web/src/resource";
import {
  appQueryKeys,
  workRefreshInterval,
} from "../../generated/web/src/query";
import {
  ActionFeedback,
  EmptyState,
  LoadingState,
  formatDateTime,
  formatNumber,
  statusLabel,
  t,
  useActionFeedback,
} from "../../generated/web/src/ui";

export function ReportStudioPage() {
  const client = useQueryClient();
  const reportScope = appQueryKeys.reports.all();
  const [title, setTitle] = useState("");
  const [source, setSource] = useState("");
  const [error, setError] = useState("");
  const action = useActionFeedback(errorMessage);
  const request = useRef<{ payload: string; key: string } | null>(null);
  const files = useQuery({
    queryKey: ["files", "studio"],
    queryFn: ({ signal }) => fileApi.list(1, 100, { signal }),
  });
  const runs = useQuery({
    queryKey: appQueryKeys.reports.runs(1, 10),
    queryFn: ({ signal }) => reportApi.list(1, 10, { signal }),
    refetchInterval: (query) => workRefreshInterval(query.state.data?.data),
    refetchOnWindowFocus: true,
  });
  const create = useMutation({
    mutationFn: async () => {
      const blob = await fileApi.download(source);
      const summary = await blob.text();
      if (summary.length > 10_000)
        throw new Error(t("Use a text input with at most 10,000 characters."));
      const data = { title: title.trim(), summary };
      const payload = JSON.stringify(data);
      if (request.current?.payload !== payload)
        request.current = { payload, key: crypto.randomUUID() };
      return reportApi.create("team-summary", data, request.current.key);
    },
    onSuccess: async () => {
      request.current = null;
      setTitle("");
      await Promise.all([
        client.invalidateQueries({ queryKey: reportScope }),
        client.invalidateQueries({ queryKey: ["billing"] }),
        client.invalidateQueries({ queryKey: appQueryKeys.product.all }),
      ]);
    },
  });
  return (
    <main className="page">
      <div className="page-heading">
        <div>
          <p className="product-eyebrow">TEAM REPORTS</p>
          <h1>{t("Your report studio")}</h1>
          <p>
            {t(
              "Upload a text or JSON file, give your report a title and let the worker build it.",
            )}
          </p>
        </div>
        <Link to="/billing">{t("Plan and allowance")}</Link>
      </div>
      <FileUpload
        onUploaded={(file) => {
          if (["text/plain", "application/json"].includes(file.content_type)) {
            setSource(file.id);
            setError("");
          } else {
            setSource("");
            setError(
              t(
                "The file is saved to your library. Choose a text or JSON file as report input.",
              ),
            );
          }
        }}
      />
      <BillingAccess entitlement="reports">
        <section className="panel">
          <h2>{t("Build a team report")}</h2>
          <form
            className="auth-form"
            onSubmit={(event) => {
              event.preventDefault();
              if (!create.isPending) create.mutate();
            }}
          >
            <label>
              {t("Report title")}
              <input
                required
                maxLength={120}
                value={title}
                disabled={create.isPending}
                onChange={(event) => setTitle(event.target.value)}
              />
            </label>
            <label>
              {t("Input file")}
              <select
                required
                value={source}
                disabled={create.isPending || files.isPending}
                onChange={(event) => setSource(event.target.value)}
              >
                <option value="">{t("Select a text or JSON file")}</option>
                {files.data?.data
                  .filter((file) =>
                    ["text/plain", "application/json"].includes(
                      file.content_type,
                    ),
                  )
                  .map((file) => (
                    <option key={file.id} value={file.id}>
                      {file.original_name}
                    </option>
                  ))}
              </select>
            </label>
            <button
              className="primary-button"
              disabled={create.isPending || !source || !title.trim()}
            >
              {t(create.isPending ? "Submitting..." : "Generate report")}
            </button>
          </form>
          <ActionFeedback
            error={create.error ? errorMessage(create.error) : ""}
            notice={
              create.isSuccess
                ? t("Report submitted. Follow its progress below.")
                : ""
            }
          />
        </section>
      </BillingAccess>
      <ActionFeedback
        error={
          error ||
          action.error ||
          (files.error || runs.error
            ? errorMessage(files.error ?? runs.error)
            : "")
        }
        notice={action.notice}
        onRetry={
          files.error || runs.error
            ? () => {
                void files.refetch();
                void runs.refetch();
              }
            : undefined
        }
      />
      <section className="panel">
        <div className="page-heading">
          <h2>{t("Recent reports")}</h2>
          <Link to="/results">{t("All results")}</Link>
        </div>
        {runs.isPending && <LoadingState />}
        {runs.data?.data.length === 0 && (
          <EmptyState
            title={t("No reports yet")}
            description={t("Upload your first input to get started.")}
          />
        )}
        {runs.data?.data.map((run) => (
          <article
            key={run.id}
            className="studio-run"
            aria-busy={action.isPending(run.id)}
          >
            <div>
              <strong>{run.template}</strong>
              <p>{formatDateTime(run.created_at)}</p>
              <p>
                {statusLabel(run.stage)} · {formatNumber(run.progress)}%
              </p>
              {run.error_code && (
                <p>
                  {t("Task failed. Retry it or share this code with support.")}{" "}
                  <code>{run.error_code}</code>
                </p>
              )}
            </div>
            {run.stage === "succeeded" && (
              <button
                className="secondary-button"
                type="button"
                disabled={action.isPending(run.id)}
                onClick={() =>
                  void action.run(
                    run.id,
                    async () =>
                      triggerDownload(
                        await reportApi.download(run.id),
                        "team-report.pdf",
                      ),
                    t("Download started"),
                  )
                }
              >
                {t("Download PDF")}
              </button>
            )}
          </article>
        ))}
      </section>
    </main>
  );
}
