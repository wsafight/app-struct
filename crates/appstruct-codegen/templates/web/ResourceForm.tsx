import { ArrowLeft, RefreshCw, Save } from "lucide-react";
import { FormEvent, useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import type { FieldDefinition, ResourceDefinition, ResourceInput, ResourceRecord } from "../resource";
import type { AppStructRegistry, FieldComponentProps } from "../generated/registry";
import { canAccessResource, errorMessage, fieldErrors, useCanAccess, useResourceActor } from "../resource";

type FormValues = Record<string, string | boolean>;

export function ResourceForm({ resource, resources, registry }: { resource: ResourceDefinition; resources: ResourceDefinition[]; registry?: AppStructRegistry }) {
  const { id } = useParams();
  const navigate = useNavigate();
  const editing = id !== undefined;
  const canSubmit = useCanAccess(resource, editing ? "update" : "create");
  const [values, setValues] = useState<FormValues>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [pageError, setPageError] = useState("");
  const [loading, setLoading] = useState(editing);
  const [saving, setSaving] = useState(false);
  const [conflict, setConflict] = useState(false);
  const fields = resource.fields.filter((field) => !field.readOnly && !field.primaryKey);

  useEffect(() => {
    void loadRecord();
  }, [id, resource]);

  async function loadRecord() {
    if (!id || !canSubmit) return;
    setLoading(true);
    setConflict(false);
    try {
      const record = await resource.api.get(id);
      setValues(Object.fromEntries(fields.map((field) => [field.name, toFormValue(record[field.name], field)])));
    } catch (reason) {
      setPageError(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  if (!canSubmit) return <main className="page"><div className="alert" role="alert">You do not have permission to change this resource.</div></main>;

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setErrors({});
    setPageError("");
    try {
      const entries = fields.filter((field) => editing || values[field.name] !== undefined).map((field) => [field.name, toApiValue(values[field.name], field)]);
      const input = Object.fromEntries(entries) as ResourceInput;
      if (id) await resource.api.update(id, input); else await resource.api.create(input);
      navigate(id ? `/${resource.slug}/${encodeURIComponent(id)}` : `/${resource.slug}`);
    } catch (reason) {
      setErrors(fieldErrors(reason));
      setPageError(errorMessage(reason));
      setConflict((reason as { code?: string } | undefined)?.code === "CONCURRENT_MODIFICATION");
    } finally {
      setSaving(false);
    }
  }

  return <main className="page form-page">
    <div className="page-heading"><div><Link className="back-link" to={`/${resource.slug}`}><ArrowLeft size={16} /> {resource.label}</Link><h1>{editing ? "Edit" : "Add"} {resource.label}</h1></div></div>
    {pageError && <div className="alert" role="alert">{pageError}{conflict && <button type="button" className="secondary-button" onClick={() => void loadRecord()}><RefreshCw size={16} /> Reload latest</button>}</div>}
    {loading ? <div className="form-frame">Loading...</div> : <form className="form-frame" onSubmit={(event) => void submit(event)}><div className="form-grid">
      {fields.map((field) => <FieldControl key={field.name} field={field} resources={resources} registry={registry} value={values[field.name]} error={errors[field.name]} onChange={(value) => setValues((current) => ({ ...current, [field.name]: value }))} />)}
    </div><div className="form-actions"><Link className="secondary-button" to={`/${resource.slug}`}>Cancel</Link><button className="primary-button" disabled={saving}><Save size={17} /> {saving ? "Saving..." : "Save"}</button></div></form>}
  </main>;
}

function FieldControl({ field, resources, registry, value, error, onChange }: { field: FieldDefinition; resources: ResourceDefinition[]; registry?: AppStructRegistry; value: string | boolean | undefined; error?: string; onChange(value: string | boolean): void }) {
  const id = `field-${field.name}`;
  if (field.uiComponent) {
    const components = registry?.fields as Record<string, React.ComponentType<FieldComponentProps>> | undefined;
    const Component = components?.[String(field.uiComponent)];
    return <div className="field"><label>{field.label}{field.required && <span aria-hidden> *</span>}</label>{Component ? <Component label={field.label} required={field.required} value={value} error={error} readOnly={false} onChange={onChange} /> : <div className="alert" role="alert">Field renderer unavailable</div>}</div>;
  }
  if (field.kind === "boolean") return <label className="checkbox-field"><input id={id} type="checkbox" checked={Boolean(value)} onChange={(event) => onChange(event.target.checked)} /> <span>{field.label}</span>{error && <small>{error}</small>}</label>;
  if (field.kind === "relation") return <RelationSelect id={id} field={field} target={resources.find((resource) => resource.id === field.relation)} value={String(value ?? "")} error={error} onChange={onChange} />;
  const common = { id, name: field.name, required: field.required, value: String(value ?? ""), onChange: (event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>) => onChange(event.target.value), "aria-invalid": Boolean(error), "aria-describedby": error ? `${id}-error` : undefined };
  return <div className="field"><label htmlFor={id}>{field.label}{field.required && <span aria-hidden> *</span>}</label>
    {field.kind === "enum" ? <select {...common}><option value="">Select</option>{field.values?.map((item) => <option key={item}>{item}</option>)}</select>
      : field.kind === "text" || field.kind === "json" ? <textarea {...common} rows={field.kind === "json" ? 7 : 4} />
      : <input {...common} type={inputType(field.kind)} min={field.minimum} max={field.maximum} />}
    {error && <small id={`${id}-error`} className="field-error">{error}</small>}
  </div>;
}

function RelationSelect({ id, field, target, value, error, onChange }: { id: string; field: FieldDefinition; target?: ResourceDefinition; value: string; error?: string; onChange(value: string): void }) {
  const actor = useResourceActor();
  const [options, setOptions] = useState<ResourceRecord[]>([]);
  const [loadError, setLoadError] = useState("");
  useEffect(() => {
    if (!target || !canAccessResource(target, "list", actor)) return;
    target.api.list({ page_size: 100 }).then((response) => setOptions(response.data)).catch((reason) => setLoadError(errorMessage(reason)));
  }, [actor, target]);
  const labelField = target?.fields.find((item) => !item.primaryKey && (item.kind === "string" || item.kind === "text"));
  return <div className="field"><label htmlFor={id}>{field.label}{field.required && <span aria-hidden> *</span>}</label><select id={id} required={field.required} value={value} onChange={(event) => onChange(event.target.value)} aria-invalid={Boolean(error)}><option value="">Select</option>{options.map((record) => { const optionValue = String(record[target?.primaryKey ?? "id"]); return <option key={optionValue} value={optionValue}>{String(record[labelField?.name ?? target?.primaryKey ?? "id"])}</option>; })}</select>{(error || loadError) && <small className="field-error">{error || loadError}</small>}</div>;
}

function inputType(kind: FieldDefinition["kind"]): string {
  if (kind === "integer" || kind === "bigint" || kind === "decimal") return "number";
  if (kind === "date") return "date";
  if (kind === "datetime") return "datetime-local";
  return "text";
}

function toFormValue(value: unknown, field: FieldDefinition): string | boolean {
  if (field.kind === "boolean") return Boolean(value);
  if (field.kind === "json") return value == null ? "" : JSON.stringify(value, null, 2);
  if (field.kind === "datetime" && typeof value === "string") return value.slice(0, 16);
  return value == null ? "" : String(value);
}

function toApiValue(value: string | boolean | undefined, field: FieldDefinition): unknown {
  if (value === "" || value === undefined) return field.required ? value : null;
  if (field.kind === "integer" || field.kind === "bigint") return Number(value);
  if (field.kind === "boolean") return Boolean(value);
  if (field.kind === "json") return JSON.parse(String(value));
  if (field.kind === "datetime") return new Date(String(value)).toISOString();
  return value;
}
