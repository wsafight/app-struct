import { useForm } from "@tanstack/react-form";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, RefreshCw, Save } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ComponentType } from "react";
import { z } from "zod";
import type { AppStructRegistry, FieldComponentProps } from "../generated/registry";
import { Link, useNavigate, useParams } from "../navigation";
import { resourceQueryKeys } from "../query";
import type { FieldDefinition, ResourceDefinition, ResourceInput, ResourceRecord } from "../resource";
import { canAccessResource, canAccessRule, errorMessage, fieldErrors, useCanAccess, useResourceActor } from "../resource";

type FormValue = string | boolean;
type FormValues = Record<string, FormValue>;

export function ResourceForm({ resource, resources, registry }: { resource: ResourceDefinition; resources: ResourceDefinition[]; registry?: AppStructRegistry }) {
  const { id } = useParams();
  const navigate = useNavigate();
  const actor = useResourceActor();
  const queryClient = useQueryClient();
  const editing = id !== undefined;
  const canSubmit = useCanAccess(resource, editing ? "update" : "create");
  const [serverErrors, setServerErrors] = useState<Record<string, string>>({});
  const [conflict, setConflict] = useState(false);
  const initializedRecord = useRef("");
  const fields = useMemo(
    () => resource.fields.filter((field) => !field.readOnly && !field.primaryKey && canAccessRule(field.writeAccess ?? { mode: "public" }, actor)),
    [actor, resource],
  );
  const defaultValues = useMemo(() => emptyFormValues(fields), [fields]);
  const validationSchema = useMemo(() => buildValidationSchema(fields), [fields]);

  const recordQuery = useQuery({
    queryKey: resourceQueryKeys.detail(resource.id, id ?? ""),
    queryFn: () => resource.api.get(id!),
    enabled: Boolean(editing && id && canSubmit),
  });

  const saveMutation = useMutation({
    mutationFn: (input: ResourceInput) => id ? resource.api.update(id, input) : resource.api.create(input),
    onSuccess: async (record) => {
      if (id) queryClient.setQueryData(resourceQueryKeys.detail(resource.id, id), record);
      await queryClient.invalidateQueries({ queryKey: resourceQueryKeys.all(resource.id) });
      await navigate(id ? `/${resource.slug}/${encodeURIComponent(id)}` : `/${resource.slug}`, { replace: true });
    },
    onError: (reason) => {
      setServerErrors(fieldErrors(reason));
      setConflict((reason as { code?: string } | undefined)?.code === "CONCURRENT_MODIFICATION");
    },
  });

  const form = useForm({
    defaultValues,
    validators: { onSubmit: validationSchema },
    onSubmit: async ({ value }) => {
      setServerErrors({});
      setConflict(false);
      saveMutation.reset();
      const entries = fields
        .filter((field) => editing || form.getFieldMeta(field.name)?.isTouched)
        .map((field) => [field.name, toApiValue(value[field.name], field)]);
      try {
        await saveMutation.mutateAsync(Object.fromEntries(entries) as ResourceInput);
      } catch {
        // Mutation callbacks expose the server errors in the form.
      }
    },
  });

  useEffect(() => {
    const routeKey = `${resource.id}:${id ?? "new"}`;
    if (editing) {
      if (recordQuery.data && initializedRecord.current !== routeKey) {
        form.reset(recordFormValues(recordQuery.data, fields));
        initializedRecord.current = routeKey;
      }
    } else if (initializedRecord.current !== routeKey) {
      form.reset(defaultValues);
      initializedRecord.current = routeKey;
    }
  }, [defaultValues, editing, fields, form, id, recordQuery.data, resource.id]);

  if (!canSubmit) {
    return <main className="page"><div className="alert" role="alert">You do not have permission to change this resource.</div></main>;
  }

  async function reloadRecord() {
    setConflict(false);
    saveMutation.reset();
    const result = await recordQuery.refetch();
    if (result.data) form.reset(recordFormValues(result.data, fields));
  }

  function clearServerError(field: string) {
    setServerErrors((current) => {
      if (!(field in current)) return current;
      const next = { ...current };
      delete next[field];
      return next;
    });
  }

  const pageError = saveMutation.error ? errorMessage(saveMutation.error) : recordQuery.error ? errorMessage(recordQuery.error) : "";
  return <main className="page form-page">
    <div className="page-heading"><div><Link className="back-link" to={`/${resource.slug}`}><ArrowLeft size={16} /> {resource.label}</Link><h1>{editing ? "Edit" : "Add"} {resource.label}</h1></div></div>
    {pageError && <div className="alert" role="alert">{pageError}{conflict && <button type="button" className="secondary-button" onClick={() => void reloadRecord()}><RefreshCw size={16} /> Reload latest</button>}</div>}
    {recordQuery.isPending && editing ? <div className="form-frame">Loading...</div> : <form className="form-frame" onSubmit={(event) => { event.preventDefault(); event.stopPropagation(); void form.handleSubmit(); }}><div className="form-grid">
      {fields.map((field) => <form.Field key={field.name} name={field.name}>{(formField) => <FieldControl field={field} resources={resources} registry={registry} value={formField.state.value} error={serverErrors[field.name] ?? validationMessage(formField.state.meta.errors)} onBlur={formField.handleBlur} onChange={(value) => { clearServerError(field.name); formField.handleChange(value); }} />}</form.Field>)}
    </div><div className="form-actions"><Link className="secondary-button" to={`/${resource.slug}`}>Cancel</Link><form.Subscribe selector={(state) => [state.canSubmit, state.isSubmitting] as const}>{([ready, submitting]) => <button className="primary-button" disabled={!ready || submitting}><Save size={17} /> {submitting ? "Saving..." : "Save"}</button>}</form.Subscribe></div></form>}
  </main>;
}

function FieldControl({ field, resources, registry, value, error, onBlur, onChange }: { field: FieldDefinition; resources: ResourceDefinition[]; registry?: AppStructRegistry; value: FormValue; error?: string; onBlur(): void; onChange(value: FormValue): void }) {
  const id = `field-${field.name}`;
  if (field.uiComponent) {
    const components = registry?.fields as Record<string, ComponentType<FieldComponentProps>> | undefined;
    const Component = components?.[String(field.uiComponent)];
    return <div className="field"><label>{field.label}{field.required && <span aria-hidden> *</span>}</label>{Component ? <Component label={field.label} required={field.required} value={value} error={error} readOnly={false} onChange={onChange} /> : <div className="alert" role="alert">Field renderer unavailable</div>}</div>;
  }
  if (field.kind === "boolean") return <label className="checkbox-field"><input id={id} type="checkbox" checked={Boolean(value)} onBlur={onBlur} onChange={(event) => onChange(event.target.checked)} /> <span>{field.label}</span>{error && <small>{error}</small>}</label>;
  if (field.kind === "relation") return <RelationSelect id={id} field={field} target={resources.find((resource) => resource.id === field.relation)} value={String(value ?? "")} error={error} onBlur={onBlur} onChange={onChange} />;
  const common = { id, name: field.name, required: field.required, value: String(value ?? ""), onBlur, onChange: (event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>) => onChange(event.target.value), "aria-invalid": Boolean(error), "aria-describedby": error ? `${id}-error` : undefined };
  return <div className="field"><label htmlFor={id}>{field.label}{field.required && <span aria-hidden> *</span>}</label>
    {field.kind === "enum" ? <select {...common}><option value="">Select</option>{field.values?.map((item) => <option key={item}>{item}</option>)}</select>
      : field.kind === "text" || field.kind === "json" ? <textarea {...common} rows={field.kind === "json" ? 7 : 4} />
      : <input {...common} type={inputType(field.kind)} min={field.minimum} max={field.maximum} />}
    {error && <small id={`${id}-error`} className="field-error">{error}</small>}
  </div>;
}

function RelationSelect({ id, field, target, value, error, onBlur, onChange }: { id: string; field: FieldDefinition; target?: ResourceDefinition; value: string; error?: string; onBlur(): void; onChange(value: string): void }) {
  const actor = useResourceActor();
  const canLoad = Boolean(target && canAccessResource(target, "list", actor));
  const optionsQuery = useQuery({
    queryKey: resourceQueryKeys.options(target?.id ?? "unavailable"),
    queryFn: () => target!.api.list({ page_size: 100 }),
    enabled: canLoad,
  });
  const labelField = target?.fields.find((item) => !item.primaryKey && (item.kind === "string" || item.kind === "text"));
  const loadError = optionsQuery.error ? errorMessage(optionsQuery.error) : "";
  return <div className="field"><label htmlFor={id}>{field.label}{field.required && <span aria-hidden> *</span>}</label><select id={id} required={field.required} value={value} onBlur={onBlur} onChange={(event) => onChange(event.target.value)} aria-invalid={Boolean(error || loadError)} disabled={optionsQuery.isPending && canLoad}><option value="">Select</option>{optionsQuery.data?.data.map((record) => { const optionValue = String(record[target?.primaryKey ?? "id"]); return <option key={optionValue} value={optionValue}>{String(record[labelField?.name ?? target?.primaryKey ?? "id"])}</option>; })}</select>{(error || loadError) && <small className="field-error">{error || loadError}</small>}</div>;
}

function emptyFormValues(fields: FieldDefinition[]): FormValues {
  return Object.fromEntries(fields.map((field) => [field.name, field.kind === "boolean" ? false : ""])) as FormValues;
}

function recordFormValues(record: ResourceRecord, fields: FieldDefinition[]): FormValues {
  return Object.fromEntries(fields.map((field) => [field.name, toFormValue(record[field.name], field)])) as FormValues;
}

function buildValidationSchema(fields: FieldDefinition[]): z.ZodType<FormValues, FormValues> {
  return z.object(Object.fromEntries(fields.map((field) => [field.name, fieldSchema(field)]))) as z.ZodType<FormValues, FormValues>;
}

function fieldSchema(field: FieldDefinition): z.ZodType<FormValue, FormValue> {
  if (field.kind === "boolean") return z.boolean();
  return z.string().superRefine((value, context) => {
    if (!value) {
      if (field.required) context.addIssue({ code: "custom", message: `${field.label} is required` });
      return;
    }
    if (["integer", "bigint", "decimal"].includes(field.kind)) {
      const number = Number(value);
      if (!Number.isFinite(number)) context.addIssue({ code: "custom", message: `${field.label} must be a number` });
      if ((field.kind === "integer" || field.kind === "bigint") && !Number.isInteger(number)) context.addIssue({ code: "custom", message: `${field.label} must be a whole number` });
      if (field.minimum !== undefined && number < Number(field.minimum)) context.addIssue({ code: "custom", message: `${field.label} must be at least ${field.minimum}` });
      if (field.maximum !== undefined && number > Number(field.maximum)) context.addIssue({ code: "custom", message: `${field.label} must be at most ${field.maximum}` });
    }
    if (field.kind === "uuid" && !z.uuid().safeParse(value).success) context.addIssue({ code: "custom", message: `${field.label} must be a valid UUID` });
    if (field.kind === "json") {
      try { JSON.parse(value); } catch { context.addIssue({ code: "custom", message: `${field.label} must contain valid JSON` }); }
    }
    if ((field.kind === "date" || field.kind === "datetime") && Number.isNaN(Date.parse(value))) context.addIssue({ code: "custom", message: `${field.label} must be a valid date` });
  });
}

function validationMessage(errors: unknown[]): string | undefined {
  for (const error of errors.flat(Infinity)) {
    if (typeof error === "string") return error;
    if (error && typeof error === "object" && "message" in error && typeof error.message === "string") return error.message;
  }
  return undefined;
}

function inputType(kind: FieldDefinition["kind"]): string {
  if (kind === "integer" || kind === "bigint" || kind === "decimal") return "number";
  if (kind === "date") return "date";
  if (kind === "datetime") return "datetime-local";
  return "text";
}

function toFormValue(value: unknown, field: FieldDefinition): FormValue {
  if (field.kind === "boolean") return Boolean(value);
  if (field.kind === "json") return value == null ? "" : JSON.stringify(value, null, 2);
  if (field.kind === "datetime" && typeof value === "string") return value.slice(0, 16);
  return value == null ? "" : String(value);
}

function toApiValue(value: FormValue | undefined, field: FieldDefinition): unknown {
  if (value === "" || value === undefined) return field.required ? value : null;
  if (field.kind === "integer" || field.kind === "bigint") return Number(value);
  if (field.kind === "boolean") return Boolean(value);
  if (field.kind === "json") return JSON.parse(String(value));
  if (field.kind === "datetime") return new Date(String(value)).toISOString();
  return value;
}
