import { formatDateTime } from "../ui";
import { ActionFeedback, statusLabel, t, useActionFeedback } from "../ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Download, FileText, X } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import {
  reportApi,
  type ReportInputMap,
  type ReportRun,
  type ReportTemplate,
  type ReportTemplateName,
} from "../generated/client";
import { appQueryKeys, workRefreshInterval } from "../query";
import { errorMessage, triggerDownload } from "../resource";
__BILLING_IMPORT__;

export function ReportPage() {
  const client = useQueryClient();
  const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<ReportTemplateName | "">("");
  const [values, setValues] = useState<Record<string, unknown>>({});
  const action = useActionFeedback(errorMessage);
  const scope = appQueryKeys.reports.all();
  const request = useRef<{ payload: string; key: string } | null>(null);
  const pageSize = 25;
  const templates = useQuery({
    queryKey: appQueryKeys.reports.templates(),
    queryFn: ({ signal }) => reportApi.templates({ signal }),
  });
  const runs = useQuery({
    queryKey: appQueryKeys.reports.runs(page, pageSize),
    queryFn: ({ signal }) => reportApi.list(page, pageSize, { signal }),
    refetchInterval: (query) => workRefreshInterval(query.state.data?.data),
    refetchOnWindowFocus: true,
  });
  const create = useMutation({
    mutationFn: ({
      template,
      data,
    }: {
      template: ReportTemplateName;
      data: Record<string, unknown>;
    }) => {
      const payload = JSON.stringify({ template, data });
      if (request.current?.payload !== payload) {
        request.current = { payload, key: crypto.randomUUID() };
      }
      return reportApi.create(
        template,
        data as ReportInputMap[ReportTemplateName],
        request.current.key,
      );
    },
    onSuccess: async () => {
      request.current = null;
      setValues({});
      await client.invalidateQueries({ queryKey: scope });
      await client.invalidateQueries({ queryKey: ["billing"] });
      await client.invalidateQueries({ queryKey: appQueryKeys.product.all });
    },
  });
  const selectedTemplate = useMemo(
    () => templates.data?.find((template) => template.name === selected),
    [selected, templates.data],
  );
  const inputValues = selectedTemplate
    ? withBooleanDefaults(selectedTemplate, values)
    : values;
  const error = create.error ?? templates.error ?? runs.error;
  const total = runs.data?.meta.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / pageSize));

  async function download(run: ReportRun) {
    await action.run(
      run.id,
      async () => {
        const blob = await reportApi.download(run.id);
        triggerDownload(blob, `${run.template}-${run.id}.pdf`);
      },
      t("Download started"),
    );
  }
  async function cancel(id: string) {
    await action.run(
      id,
      async () => {
        await reportApi.cancel(id);
        await Promise.all([
          client.invalidateQueries({ queryKey: scope }),
          client.invalidateQueries({ queryKey: ["billing"] }),
          client.invalidateQueries({ queryKey: appQueryKeys.product.all }),
        ]);
      },
      t("Task cancelled"),
    );
  }

  return (
    <main className="page report-page">
      <div className="page-heading">
        <div>
          <h1>{t("Reports")}</h1>
          <p>
            {total} {t("runs")}
          </p>
        </div>
        <FileText size={22} aria-hidden />
      </div>
      <ActionFeedback
        error={error ? errorMessage(error) : action.error}
        notice={action.notice}
        onRetry={
          runs.error || templates.error
            ? () => {
                void runs.refetch();
                void templates.refetch();
              }
            : undefined
        }
      />
      __BILLING_OPEN__
      <section className="report-create" aria-labelledby="new-report-heading">
        <h2 id="new-report-heading">{t("New report")}</h2>
        <div className="report-create-grid">
          <label>
            {" "}
            {t("Template")}
            <select
              value={selected}
              onChange={(event) => {
                setSelected(event.target.value as ReportTemplateName | "");
                setValues({});
              }}
            >
              <option value=""> {t("Select template")}</option>
              {(templates.data ?? []).map((template) => (
                <option
                  key={`${template.name}:${template.version}`}
                  value={template.name}
                >
                  {template.name} v{template.version}
                </option>
              ))}
            </select>
          </label>
          {selectedTemplate && (
            <SchemaFields
              template={selectedTemplate}
              values={inputValues}
              onChange={setValues}
            />
          )}
          <button
            type="button"
            className="primary-button report-run-button"
            disabled={
              !selectedTemplate ||
              create.isPending ||
              !supportedInput(selectedTemplate) ||
              !requiredPresent(selectedTemplate, inputValues)
            }
            onClick={() =>
              selectedTemplate &&
              create.mutate({
                template: selectedTemplate.name,
                data: inputValues,
              })
            }
          >
            <FileText size={16} /> {t("Generate")}
          </button>
        </div>
      </section>
      __BILLING_CLOSE__
      <section className="table-frame report-table">
        <table>
          <thead>
            <tr>
              <th> {t("Template")}</th>
              <th> {t("Created")} </th>
              <th>{t("Status")}</th>
              <th>{t("Progress")}</th>
              <th>{t("Expires")}</th>
              <th aria-label={t("Actions")} />
            </tr>
          </thead>
          <tbody>
            {runs.isPending && (
              <tr>
                <td colSpan={6} className="empty">
                  {t("Loading...")}
                </td>
              </tr>
            )}
            {!runs.isPending && (runs.data?.data.length ?? 0) === 0 && (
              <tr>
                <td colSpan={6} className="empty">
                  {t("No report runs")}
                </td>
              </tr>
            )}
            {(runs.data?.data ?? []).map((run) => (
              <tr key={run.id}>
                <td>
                  <strong>{run.template}</strong>
                  <small>v{run.template_version}</small>
                </td>
                <td>{formatDate(run.created_at)}</td>
                <td>
                  <span className={`report-status ${run.stage}`}>
                    {statusLabel(run.stage)}
                  </span>
                  {run.error_code && <small>{run.error_code}</small>}
                </td>
                <td>
                  <progress
                    max={100}
                    value={run.progress}
                    aria-label={`${run.progress}%`}
                  />
                  <span>{run.progress}%</span>
                </td>
                <td>{formatDate(run.expires_at)}</td>
                <td>
                  <div className="row-actions">
                    {active(run) && (
                      <button
                        type="button"
                        className="icon-button"
                        title={t("Cancel report")}
                        aria-label={t("Cancel {0}", { "0": run.template })}
                        disabled={action.isPending(run.id)}
                        onClick={() => void cancel(run.id)}
                      >
                        <X size={16} />
                      </button>
                    )}
                    {run.stage === "succeeded" && (
                      <button
                        type="button"
                        className="icon-button"
                        title={t("Download PDF")}
                        aria-label={t("Download {0}", { "0": run.template })}
                        disabled={action.isPending(run.id)}
                        onClick={() => void download(run)}
                      >
                        <Download size={16} />
                      </button>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
      <div className="pagination">
        <span>{t("Page {0} of {1}", { 0: page, 1: pages })}</span>
        <div>
          <button
            className="icon-button"
            disabled={page <= 1}
            onClick={() => setPage((value) => value - 1)}
            aria-label={t("Previous page")}
          >
            <ChevronLeft size={17} />
          </button>
          <button
            className="icon-button"
            disabled={page >= pages}
            onClick={() => setPage((value) => value + 1)}
            aria-label={t("Next page")}
          >
            <ChevronRight size={17} />
          </button>
        </div>
      </div>
    </main>
  );
}

type SchemaField =
  | { type: "string" | "number" | "boolean" }
  | { type: "enum"; options: string[] }
  | { type: "unsupported" };

function SchemaFields({
  template,
  values,
  onChange,
}: {
  template: ReportTemplate;
  values: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
}) {
  const properties = objectValue(template.input_schema.properties);
  const required = new Set(
    Array.isArray(template.input_schema.required)
      ? template.input_schema.required
      : [],
  );
  return (
    <>
      {Object.entries(properties).map(([name, rawSchema]) => {
        const field = schemaField(rawSchema);
        const set = (value: unknown) => onChange({ ...values, [name]: value });
        const clear = () => {
          const next = { ...values };
          delete next[name];
          onChange(next);
        };
        if (field.type === "unsupported")
          return (
            <label key={name}>
              {name}
              <input type="text" value="" disabled />
              <small className="field-error">
                {t("Unsupported report input type")}
              </small>
            </label>
          );
        if (field.type === "enum")
          return (
            <label key={name}>
              {name}
              <select
                required={required.has(name)}
                value={String(values[name] ?? "")}
                onChange={(event) =>
                  event.target.value === "" ? clear() : set(event.target.value)
                }
              >
                <option value="">{t("Select...")}</option>
                {field.options.map((option) => (
                  <option key={option} value={option}>
                    {option}
                  </option>
                ))}
              </select>
            </label>
          );
        if (field.type === "boolean")
          return (
            <label key={name} className="checkbox-field">
              <input
                type="checkbox"
                checked={Boolean(values[name])}
                onChange={(event) => set(event.target.checked)}
              />
              {name}
            </label>
          );
        const numeric = field.type === "number";
        return (
          <label key={name}>
            {name}
            <input
              type={numeric ? "number" : "text"}
              required={required.has(name)}
              value={String(values[name] ?? "")}
              onChange={(event) => {
                const value = event.target.value;
                if (numeric && value === "") clear();
                else set(numeric ? Number(value) : value);
              }}
            />
          </label>
        );
      })}
    </>
  );
}

function schemaField(value: unknown): SchemaField {
  const schema = objectValue(value);
  if (Array.isArray(schema.enum)) {
    const options = schema.enum.filter(
      (option): option is string => typeof option === "string",
    );
    return options.length > 0 && options.length === schema.enum.length
      ? { type: "enum", options }
      : { type: "unsupported" };
  }
  switch (schema.type) {
    case "string":
      return { type: "string" };
    case "integer":
    case "number":
      return { type: "number" };
    case "boolean":
      return { type: "boolean" };
    default:
      return { type: "unsupported" };
  }
}

function supportedInput(template: ReportTemplate): boolean {
  return Object.values(objectValue(template.input_schema.properties)).every(
    (schema) => schemaField(schema).type !== "unsupported",
  );
}

function withBooleanDefaults(
  template: ReportTemplate,
  values: Record<string, unknown>,
): Record<string, unknown> {
  const next = { ...values };
  for (const [name, schema] of Object.entries(
    objectValue(template.input_schema.properties),
  )) {
    if (schemaField(schema).type === "boolean" && next[name] === undefined)
      next[name] = false;
  }
  return next;
}

function requiredPresent(
  template: ReportTemplate,
  values: Record<string, unknown>,
): boolean {
  const required = Array.isArray(template.input_schema.required)
    ? template.input_schema.required
    : [];
  return required.every(
    (name) =>
      typeof name === "string" &&
      values[name] !== undefined &&
      values[name] !== "",
  );
}
function objectValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
function active(run: ReportRun): boolean {
  return (
    run.stage === "queued" ||
    run.stage === "rendering" ||
    run.stage === "publishing"
  );
}
function formatDate(value: string): string {
  return formatDateTime(value);
}
