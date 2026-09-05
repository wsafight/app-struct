import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Download, FileText, X } from "lucide-react";
import { useMemo, useState } from "react";
import {
  reportApi,
  type ReportInputMap,
  type ReportRun,
  type ReportTemplate,
  type ReportTemplateName,
} from "../generated/client";
import { appQueryKeys } from "../query";
import { errorMessage } from "../resource";

export function ReportPage() {
  const client = useQueryClient();
  const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<ReportTemplateName | "">("");
  const [values, setValues] = useState<Record<string, unknown>>({});
  const pageSize = 25;
  const templates = useQuery({
    queryKey: appQueryKeys.reports.templates,
    queryFn: () => reportApi.templates(),
  });
  const runs = useQuery({
    queryKey: appQueryKeys.reports.runs(page, pageSize),
    queryFn: () => reportApi.list(page, pageSize),
    refetchInterval: (query) =>
      query.state.data?.data.some((run) => active(run)) ? 2_000 : false,
  });
  const create = useMutation({
    mutationFn: ({ template, data }: { template: ReportTemplateName; data: Record<string, unknown> }) =>
      reportApi.create(
        template,
        data as ReportInputMap[ReportTemplateName],
        crypto.randomUUID(),
      ),
    onSuccess: async () => {
      setValues({});
      await client.invalidateQueries({ queryKey: appQueryKeys.reports.all });
    },
  });
  const cancel = useMutation({
    mutationFn: (id: string) => reportApi.cancel(id),
    onSuccess: () => client.invalidateQueries({ queryKey: appQueryKeys.reports.all }),
  });
  const selectedTemplate = useMemo(
    () => templates.data?.find((template) => template.name === selected),
    [selected, templates.data],
  );
  const error = create.error ?? cancel.error ?? templates.error ?? runs.error;
  const total = runs.data?.meta.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / pageSize));

  async function download(run: ReportRun) {
    try {
      const blob = await reportApi.download(run.id);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `${run.template}-${run.id}.pdf`;
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (requestError) {
      await client.invalidateQueries({ queryKey: appQueryKeys.reports.all });
      throw requestError;
    }
  }

  return (
    <main className="page report-page">
      <div className="page-heading">
        <div><h1>Reports</h1><p>{total} runs</p></div>
        <FileText size={22} aria-hidden />
      </div>
      {error && <div className="alert" role="alert">{errorMessage(error)}</div>}
      <section className="report-create" aria-labelledby="new-report-heading">
        <h2 id="new-report-heading">New report</h2>
        <div className="report-create-grid">
          <label>
            Template
            <select value={selected} onChange={(event) => {
              setSelected(event.target.value as ReportTemplateName | "");
              setValues({});
            }}>
              <option value="">Select template</option>
              {(templates.data ?? []).map((template) => (
                <option key={`${template.name}:${template.version}`} value={template.name}>
                  {template.name} v{template.version}
                </option>
              ))}
            </select>
          </label>
          {selectedTemplate && (
            <SchemaFields template={selectedTemplate} values={values} onChange={setValues} />
          )}
          <button
            type="button"
            className="primary-button report-run-button"
            disabled={!selectedTemplate || create.isPending || !requiredPresent(selectedTemplate, values)}
            onClick={() => selectedTemplate && create.mutate({ template: selectedTemplate.name, data: values })}
          >
            <FileText size={16} /> Generate
          </button>
        </div>
      </section>
      <section className="table-frame report-table">
        <table>
          <thead><tr><th>Template</th><th>Created</th><th>Status</th><th>Progress</th><th>Expires</th><th aria-label="Actions" /></tr></thead>
          <tbody>
            {runs.isPending && <tr><td colSpan={6} className="empty">Loading...</td></tr>}
            {!runs.isPending && (runs.data?.data.length ?? 0) === 0 && <tr><td colSpan={6} className="empty">No report runs</td></tr>}
            {(runs.data?.data ?? []).map((run) => (
              <tr key={run.id}>
                <td><strong>{run.template}</strong><small>v{run.template_version}</small></td>
                <td>{formatDate(run.created_at)}</td>
                <td><span className={`report-status ${run.stage}`}>{run.stage}</span>{run.error_code && <small>{run.error_code}</small>}</td>
                <td><progress max={100} value={run.progress} aria-label={`${run.progress}%`} /><span>{run.progress}%</span></td>
                <td>{formatDate(run.expires_at)}</td>
                <td><div className="row-actions">
                  {run.stage === "queued" && <button type="button" className="icon-button" title="Cancel report" aria-label={`Cancel ${run.template}`} disabled={cancel.isPending} onClick={() => cancel.mutate(run.id)}><X size={16} /></button>}
                  {run.stage === "succeeded" && <button type="button" className="icon-button" title="Download PDF" aria-label={`Download ${run.template}`} onClick={() => void download(run)}><Download size={16} /></button>}
                </div></td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
      <div className="pagination"><span>Page {page} of {pages}</span><div>
        <button className="icon-button" disabled={page <= 1} onClick={() => setPage((value) => value - 1)} aria-label="Previous page"><ChevronLeft size={17} /></button>
        <button className="icon-button" disabled={page >= pages} onClick={() => setPage((value) => value + 1)} aria-label="Next page"><ChevronRight size={17} /></button>
      </div></div>
    </main>
  );
}

function SchemaFields({ template, values, onChange }: {
  template: ReportTemplate;
  values: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
}) {
  const properties = objectValue(template.input_schema.properties);
  const required = new Set(Array.isArray(template.input_schema.required) ? template.input_schema.required : []);
  return <>{Object.entries(properties).map(([name, rawSchema]) => {
    const schema = objectValue(rawSchema);
    const type = typeof schema.type === "string" ? schema.type : "string";
    const set = (value: unknown) => onChange({ ...values, [name]: value });
    if (type === "boolean") return <label key={name} className="checkbox-field"><input type="checkbox" checked={Boolean(values[name])} onChange={(event) => set(event.target.checked)} />{name}</label>;
    return <label key={name}>{name}<input type={type === "integer" || type === "number" ? "number" : "text"} required={required.has(name)} value={String(values[name] ?? "")} onChange={(event) => set(type === "integer" || type === "number" ? Number(event.target.value) : event.target.value)} /></label>;
  })}</>;
}

function requiredPresent(template: ReportTemplate, values: Record<string, unknown>): boolean {
  const required = Array.isArray(template.input_schema.required) ? template.input_schema.required : [];
  return required.every((name) => typeof name === "string" && values[name] !== undefined && values[name] !== "");
}
function objectValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}
function active(run: ReportRun): boolean { return run.stage === "queued" || run.stage === "rendering" || run.stage === "publishing"; }
function formatDate(value: string): string { return new Date(value).toLocaleString(); }
