import { ArrowLeft, Save } from "lucide-react";
import { FormEvent, useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import type { FieldDefinition, ResourceDefinition, ResourceInput } from "../resource";
import { errorMessage, fieldErrors } from "../resource";

type FormValues = Record<string, string | boolean>;

export function ResourceForm({ resource }: { resource: ResourceDefinition }) {
  const { id } = useParams();
  const navigate = useNavigate();
  const editing = id !== undefined;
  const [values, setValues] = useState<FormValues>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [pageError, setPageError] = useState("");
  const [loading, setLoading] = useState(editing);
  const [saving, setSaving] = useState(false);
  const fields = resource.fields.filter((field) => !field.readOnly && !field.primaryKey);

  useEffect(() => {
    if (!id) return;
    setLoading(true);
    resource.api.get(id)
      .then((record) => setValues(Object.fromEntries(fields.map((field) => [field.name, toFormValue(record[field.name], field)]))))
      .catch((reason) => setPageError(errorMessage(reason)))
      .finally(() => setLoading(false));
  }, [id, resource]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setErrors({});
    setPageError("");
    try {
      const input = Object.fromEntries(fields.map((field) => [field.name, toApiValue(values[field.name], field)])) as ResourceInput;
      if (id) await resource.api.update(id, input);
      else await resource.api.create(input);
      navigate(`/${resource.slug}`);
    } catch (reason) {
      setErrors(fieldErrors(reason));
      setPageError(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  }

  return (
    <main className="page form-page">
      <div className="page-heading">
        <div><Link className="back-link" to={`/${resource.slug}`}><ArrowLeft size={16} /> {resource.label}</Link><h1>{editing ? "Edit" : "Add"} {resource.label}</h1></div>
      </div>
      {pageError && <div className="alert" role="alert">{pageError}</div>}
      {loading ? <div className="form-frame">Loading...</div> : (
        <form className="form-frame" onSubmit={(event) => void submit(event)}>
          <div className="form-grid">
            {fields.map((field) => <FieldControl key={field.name} field={field} value={values[field.name]} error={errors[field.name]} onChange={(value) => setValues((current) => ({ ...current, [field.name]: value }))} />)}
          </div>
          <div className="form-actions"><Link className="secondary-button" to={`/${resource.slug}`}>Cancel</Link><button className="primary-button" disabled={saving}><Save size={17} /> {saving ? "Saving..." : "Save"}</button></div>
        </form>
      )}
    </main>
  );
}

function FieldControl({ field, value, error, onChange }: { field: FieldDefinition; value: string | boolean | undefined; error?: string; onChange(value: string | boolean): void }) {
  const id = `field-${field.name}`;
  if (field.kind === "boolean") return <label className="checkbox-field"><input id={id} type="checkbox" checked={Boolean(value)} onChange={(event) => onChange(event.target.checked)} /> <span>{field.label}</span>{error && <small>{error}</small>}</label>;
  const common = { id, name: field.name, required: field.required, value: String(value ?? ""), onChange: (event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>) => onChange(event.target.value), "aria-invalid": Boolean(error), "aria-describedby": error ? `${id}-error` : undefined };
  return <div className="field"><label htmlFor={id}>{field.label}{field.required && <span aria-hidden> *</span>}</label>
    {field.kind === "enum" ? <select {...common}><option value="">Select</option>{field.values?.map((item) => <option key={item}>{item}</option>)}</select>
      : field.kind === "text" || field.kind === "json" ? <textarea {...common} rows={field.kind === "json" ? 7 : 4} />
      : <input {...common} type={inputType(field.kind)} />}
    {error && <small id={`${id}-error`} className="field-error">{error}</small>}
  </div>;
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

