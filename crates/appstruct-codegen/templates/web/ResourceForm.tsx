import { useForm } from "@tanstack/react-form";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  RefreshCw,
  Save,
} from "lucide-react";
import { useEffect, useMemo, useState, type ComponentType } from "react";
import { z } from "zod";
import { ConfirmDialog } from "../components/Dialog";
import type {
  AppStructRegistry,
  FieldComponentProps,
} from "../generated/registry";
import { Link, useNavigate, useParams, useUnsavedChanges } from "../navigation";
import { resourceQueryKeys } from "../query";
import type {
  FieldDefinition,
  ResourceDefinition,
  ResourceInput,
  ResourceRecord,
} from "../resource";
import {
  canAccessResource,
  canAccessRule,
  errorMessage,
  fieldErrors,
  useCanAccess,
  useResourceActor,
} from "../resource";

type FormValue = string | boolean;
type FormValues = Record<string, FormValue>;

export function ResourceForm({
  resource,
  resources,
  registry,
}: {
  resource: ResourceDefinition;
  resources: ResourceDefinition[];
  registry?: AppStructRegistry;
}) {
  const { id } = useParams();
  const editing = id !== undefined;
  const canSubmit = useCanAccess(resource, editing ? "update" : "create");
  const recordQuery = useQuery({
    queryKey: resourceQueryKeys.detail(resource.id, id ?? ""),
    queryFn: ({ signal }) => resource.api.get(id!, { signal }),
    enabled: Boolean(editing && id && canSubmit),
  });
  const routeKey = `${resource.id}:${id ?? "new"}`;

  if (editing && canSubmit && recordQuery.isPending) {
    return (
      <main className="page form-page">
        <div className="page-heading">
          <div>
            <Link className="back-link" to={`/${resource.slug}`}>
              <ArrowLeft size={16} /> {resource.label}
            </Link>
            <h1>Edit {resource.label}</h1>
          </div>
        </div>
        <div className="form-frame">Loading...</div>
      </main>
    );
  }

  return (
    <ResourceFormEditor
      key={`${routeKey}:${recordQuery.data ? "loaded" : "empty"}`}
      resource={resource}
      resources={resources}
      registry={registry}
      id={id}
      initialRecord={recordQuery.data}
      recordError={recordQuery.error}
      refetchRecord={async () => (await recordQuery.refetch()).data}
    />
  );
}

function ResourceFormEditor({
  resource,
  resources,
  registry,
  id,
  initialRecord,
  recordError,
  refetchRecord,
}: {
  resource: ResourceDefinition;
  resources: ResourceDefinition[];
  registry?: AppStructRegistry;
  id?: string;
  initialRecord?: ResourceRecord;
  recordError: unknown;
  refetchRecord(): Promise<ResourceRecord | undefined>;
}) {
  const navigate = useNavigate();
  const actor = useResourceActor();
  const queryClient = useQueryClient();
  const editing = id !== undefined;
  const canSubmit = useCanAccess(resource, editing ? "update" : "create");
  const [serverErrors, setServerErrors] = useState<Record<string, string>>({});
  const [conflict, setConflict] = useState(false);
  const fields = useMemo(
    () =>
      resource.fields.filter(
        (field) =>
          !field.readOnly &&
          !field.primaryKey &&
          canAccessRule(field.writeAccess ?? { mode: "public" }, actor),
      ),
    [actor, resource],
  );
  const defaultValues = useMemo(
    () =>
      initialRecord
        ? recordFormValues(initialRecord, fields)
        : emptyFormValues(fields),
    [fields, initialRecord],
  );
  const validationSchema = useMemo(
    () => buildValidationSchema(fields),
    [fields],
  );

  const saveMutation = useMutation({
    mutationFn: (input: ResourceInput) =>
      id ? resource.api.update(id, input) : resource.api.create(input),
    onSuccess: async (record) => {
      if (id)
        queryClient.setQueryData(
          resourceQueryKeys.detail(resource.id, id),
          record,
        );
      await queryClient.invalidateQueries({
        queryKey: resourceQueryKeys.all(resource.id),
      });
      await navigate(
        id
          ? `/${resource.slug}/${encodeURIComponent(id)}`
          : `/${resource.slug}`,
        { replace: true },
      );
    },
    onError: (reason) => {
      setServerErrors(fieldErrors(reason));
      setConflict(
        (reason as { code?: string } | undefined)?.code ===
          "CONCURRENT_MODIFICATION",
      );
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
        await saveMutation.mutateAsync(
          Object.fromEntries(entries) as ResourceInput,
        );
      } catch {
        // Mutation callbacks expose the server errors in the form.
      }
    },
  });

  if (!canSubmit) {
    return (
      <main className="page">
        <div className="alert" role="alert">
          You do not have permission to change this resource.
        </div>
      </main>
    );
  }

  async function reloadRecord() {
    setConflict(false);
    saveMutation.reset();
    const record = await refetchRecord();
    if (record) form.reset(recordFormValues(record, fields));
  }

  function clearServerError(field: string) {
    setServerErrors((current) => {
      if (!(field in current)) return current;
      const next = { ...current };
      delete next[field];
      return next;
    });
  }

  const pageError = saveMutation.error
    ? errorMessage(saveMutation.error)
    : recordError
      ? errorMessage(recordError)
      : "";
  return (
    <main className="page form-page">
      <div className="page-heading">
        <div>
          <Link className="back-link" to={`/${resource.slug}`}>
            <ArrowLeft size={16} /> {resource.label}
          </Link>
          <h1>
            {editing ? "Edit" : "Add"} {resource.label}
          </h1>
        </div>
      </div>
      {pageError && (
        <div className="alert" role="alert">
          {pageError}
          {conflict && (
            <button
              type="button"
              className="secondary-button"
              onClick={() => void reloadRecord()}
            >
              <RefreshCw size={16} /> Reload latest
            </button>
          )}
        </div>
      )}
      <form
        className="form-frame"
        onSubmit={(event) => {
          event.preventDefault();
          event.stopPropagation();
          void form.handleSubmit();
        }}
      >
        <form.Subscribe
          selector={(state) => [state.isDirty, state.isSubmitting] as const}
        >
          {([dirty, submitting]) => (
            <UnsavedChangesGuard enabled={dirty && !submitting} />
          )}
        </form.Subscribe>
        <div className="form-grid">
          {fields.map((field) => (
            <form.Field key={field.name} name={field.name}>
              {(formField) => (
                <FieldControl
                  field={field}
                  resources={resources}
                  registry={registry}
                  value={formField.state.value}
                  error={
                    serverErrors[field.name] ??
                    validationMessage(formField.state.meta.errors)
                  }
                  onBlur={formField.handleBlur}
                  onChange={(value) => {
                    clearServerError(field.name);
                    formField.handleChange(value);
                  }}
                />
              )}
            </form.Field>
          ))}
        </div>
        <div className="form-actions">
          <Link className="secondary-button" to={`/${resource.slug}`}>
            Cancel
          </Link>
          <form.Subscribe
            selector={(state) => [state.canSubmit, state.isSubmitting] as const}
          >
            {([ready, submitting]) => (
              <button
                className="primary-button"
                disabled={!ready || submitting}
              >
                <Save size={17} /> {submitting ? "Saving..." : "Save"}
              </button>
            )}
          </form.Subscribe>
        </div>
      </form>
    </main>
  );
}

function UnsavedChangesGuard({ enabled }: { enabled: boolean }) {
  const blocker = useUnsavedChanges(enabled);
  return (
    <ConfirmDialog
      open={blocker.blocked}
      title="Discard unsaved changes?"
      description="Your changes on this form will be lost."
      confirmLabel="Discard"
      danger
      onCancel={blocker.reset}
      onConfirm={blocker.proceed}
    />
  );
}

function FieldControl({
  field,
  resources,
  registry,
  value,
  error,
  onBlur,
  onChange,
}: {
  field: FieldDefinition;
  resources: ResourceDefinition[];
  registry?: AppStructRegistry;
  value: FormValue;
  error?: string;
  onBlur(): void;
  onChange(value: FormValue): void;
}) {
  const id = `field-${field.name}`;
  if (field.uiComponent) {
    const components = registry?.fields as
      Record<string, ComponentType<FieldComponentProps>> | undefined;
    const Component = components?.[String(field.uiComponent)];
    return (
      <div className="field">
        <label>
          {field.label}
          {field.required && <span aria-hidden> *</span>}
        </label>
        {Component ? (
          <Component
            label={field.label}
            required={field.required}
            value={value}
            error={error}
            readOnly={false}
            onChange={onChange}
          />
        ) : (
          <div className="alert" role="alert">
            Field renderer unavailable
          </div>
        )}
      </div>
    );
  }
  if (field.kind === "boolean")
    return (
      <label className="checkbox-field">
        <input
          id={id}
          type="checkbox"
          checked={Boolean(value)}
          onBlur={onBlur}
          onChange={(event) => onChange(event.target.checked)}
        />{" "}
        <span>{field.label}</span>
        {error && <small>{error}</small>}
      </label>
    );
  if (field.kind === "relation")
    return (
      <RelationSelect
        id={id}
        field={field}
        target={resources.find((resource) => resource.id === field.relation)}
        value={String(value ?? "")}
        error={error}
        onBlur={onBlur}
        onChange={onChange}
      />
    );
  const common = {
    id,
    name: field.name,
    required: field.required,
    value: String(value ?? ""),
    onBlur,
    onChange: (
      event: React.ChangeEvent<
        HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement
      >,
    ) => onChange(event.target.value),
    "aria-invalid": Boolean(error),
    "aria-describedby": error ? `${id}-error` : undefined,
  };
  return (
    <div className="field">
      <label htmlFor={id}>
        {field.label}
        {field.required && <span aria-hidden> *</span>}
      </label>
      {field.kind === "enum" ? (
        <select {...common}>
          <option value="">Select</option>
          {field.values?.map((item) => (
            <option key={item}>{item}</option>
          ))}
        </select>
      ) : field.kind === "text" || field.kind === "json" ? (
        <textarea {...common} rows={field.kind === "json" ? 7 : 4} />
      ) : (
        <input
          {...common}
          type={inputType(field.kind)}
          min={field.minimum}
          max={field.maximum}
          step={field.kind === "decimal" ? "any" : undefined}
        />
      )}
      {error && (
        <small id={`${id}-error`} className="field-error">
          {error}
        </small>
      )}
    </div>
  );
}

function RelationSelect({
  id,
  field,
  target,
  value,
  error,
  onBlur,
  onChange,
}: {
  id: string;
  field: FieldDefinition;
  target?: ResourceDefinition;
  value: string;
  error?: string;
  onBlur(): void;
  onChange(value: string): void;
}) {
  const actor = useResourceActor();
  const canLoad = Boolean(target && canAccessResource(target, "list", actor));
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const deferredSearch = useDebouncedValue(search, 250);
  const optionsQuery = useQuery({
    queryKey: resourceQueryKeys.options(
      target?.id ?? "unavailable",
      `${deferredSearch}:${page}`,
    ),
    queryFn: ({ signal }) =>
      target!.api.list(
        { page, page_size: 25, q: deferredSearch || undefined },
        { signal },
      ),
    enabled: canLoad,
    placeholderData: (previous) => previous,
  });
  const selectedQuery = useQuery({
    queryKey: resourceQueryKeys.detail(target?.id ?? "unavailable", value),
    queryFn: ({ signal }) => target!.api.get(value, { signal }),
    enabled: canLoad && Boolean(value),
  });
  const labelField = target?.fields.find(
    (item) =>
      !item.primaryKey && (item.kind === "string" || item.kind === "text"),
  );
  const loadError = optionsQuery.error ? errorMessage(optionsQuery.error) : "";
  const options = [
    ...(selectedQuery.data && value ? [selectedQuery.data] : []),
    ...(optionsQuery.data?.data ?? []),
  ].filter(
    (record, index, items) =>
      String(record[target?.primaryKey ?? "id"]) &&
      items.findIndex(
        (candidate) =>
          String(candidate[target?.primaryKey ?? "id"]) ===
          String(record[target?.primaryKey ?? "id"]),
      ) === index,
  );
  const pages = Math.max(
    1,
    Math.ceil((optionsQuery.data?.meta.total ?? 0) / 25),
  );
  const errorId = `${id}-error`;
  return (
    <div className="field">
      <label htmlFor={id}>
        {field.label}
        {field.required && <span aria-hidden> *</span>}
      </label>
      <label className="sr-only" htmlFor={`${id}-search`}>
        Search {field.label}
      </label>
      <input
        id={`${id}-search`}
        value={search}
        placeholder="Search"
        onChange={(event) => {
          setSearch(event.target.value);
          setPage(1);
        }}
      />
      <select
        id={id}
        required={field.required}
        value={value}
        onBlur={onBlur}
        onChange={(event) => onChange(event.target.value)}
        aria-invalid={Boolean(error || loadError)}
        aria-describedby={error || loadError ? errorId : undefined}
        aria-busy={optionsQuery.isFetching}
        disabled={optionsQuery.isPending && canLoad}
      >
        <option value="">Select</option>
        {options.map((record) => {
          const optionValue = String(record[target?.primaryKey ?? "id"]);
          return (
            <option key={optionValue} value={optionValue}>
              {String(record[labelField?.name ?? target?.primaryKey ?? "id"])}
            </option>
          );
        })}
      </select>
      <span className="relation-pages">
        <button
          type="button"
          className="icon-button"
          disabled={page <= 1}
          onClick={() => setPage((current) => current - 1)}
          aria-label="Previous options"
        >
          <ChevronLeft size={14} />
        </button>
        <span aria-live="polite">
          {page} / {pages}
        </span>
        <button
          type="button"
          className="icon-button"
          disabled={page >= pages}
          onClick={() => setPage((current) => current + 1)}
          aria-label="Next options"
        >
          <ChevronRight size={14} />
        </button>
      </span>
      <span className="sr-only" role="status" aria-live="polite">
        {optionsQuery.isFetching
          ? `Loading ${field.label} options`
          : `${options.length} ${field.label} options loaded`}
      </span>
      {(error || loadError) && (
        <small id={errorId} className="field-error">
          {error || loadError}
        </small>
      )}
    </div>
  );
}

function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delay);
    return () => window.clearTimeout(timer);
  }, [delay, value]);
  return debounced;
}

function emptyFormValues(fields: FieldDefinition[]): FormValues {
  return Object.fromEntries(
    fields.map((field) => [field.name, field.kind === "boolean" ? false : ""]),
  ) as FormValues;
}

function recordFormValues(
  record: ResourceRecord,
  fields: FieldDefinition[],
): FormValues {
  return Object.fromEntries(
    fields.map((field) => [field.name, toFormValue(record[field.name], field)]),
  ) as FormValues;
}

function buildValidationSchema(
  fields: FieldDefinition[],
): z.ZodType<FormValues, FormValues> {
  return z.object(
    Object.fromEntries(fields.map((field) => [field.name, fieldSchema(field)])),
  ) as z.ZodType<FormValues, FormValues>;
}

function fieldSchema(field: FieldDefinition): z.ZodType<FormValue, FormValue> {
  if (field.kind === "boolean") return z.boolean();
  return z.string().superRefine((value, context) => {
    if (!value) {
      if (field.required)
        context.addIssue({
          code: "custom",
          message: `${field.label} is required`,
        });
      return;
    }
    if (["integer", "bigint", "decimal"].includes(field.kind)) {
      const number = Number(value);
      if (!Number.isFinite(number))
        context.addIssue({
          code: "custom",
          message: `${field.label} must be a number`,
        });
      if (
        (field.kind === "integer" || field.kind === "bigint") &&
        !Number.isInteger(number)
      )
        context.addIssue({
          code: "custom",
          message: `${field.label} must be a whole number`,
        });
      if (field.minimum !== undefined && number < Number(field.minimum))
        context.addIssue({
          code: "custom",
          message: `${field.label} must be at least ${field.minimum}`,
        });
      if (field.maximum !== undefined && number > Number(field.maximum))
        context.addIssue({
          code: "custom",
          message: `${field.label} must be at most ${field.maximum}`,
        });
    }
    if (field.kind === "uuid" && !z.uuid().safeParse(value).success)
      context.addIssue({
        code: "custom",
        message: `${field.label} must be a valid UUID`,
      });
    if (field.kind === "json") {
      try {
        JSON.parse(value);
      } catch {
        context.addIssue({
          code: "custom",
          message: `${field.label} must contain valid JSON`,
        });
      }
    }
    if (
      (field.kind === "date" || field.kind === "datetime") &&
      Number.isNaN(Date.parse(value))
    )
      context.addIssue({
        code: "custom",
        message: `${field.label} must be a valid date`,
      });
  });
}

function validationMessage(errors: unknown[]): string | undefined {
  for (const error of errors.flat(Infinity)) {
    if (typeof error === "string") return error;
    if (
      error &&
      typeof error === "object" &&
      "message" in error &&
      typeof error.message === "string"
    )
      return error.message;
  }
  return undefined;
}

function inputType(kind: FieldDefinition["kind"]): string {
  if (kind === "integer" || kind === "bigint" || kind === "decimal")
    return "number";
  if (kind === "date") return "date";
  if (kind === "datetime") return "datetime-local";
  return "text";
}

function toFormValue(value: unknown, field: FieldDefinition): FormValue {
  if (field.kind === "boolean") return Boolean(value);
  if (field.kind === "json")
    return value == null ? "" : JSON.stringify(value, null, 2);
  if (field.kind === "datetime" && typeof value === "string")
    return value.slice(0, 16);
  return value == null ? "" : String(value);
}

function toApiValue(
  value: FormValue | undefined,
  field: FieldDefinition,
): unknown {
  if (value === "" || value === undefined) return field.required ? value : null;
  if (field.kind === "integer" || field.kind === "bigint") return Number(value);
  if (field.kind === "boolean") return Boolean(value);
  if (field.kind === "json") return JSON.parse(String(value));
  if (field.kind === "datetime") return new Date(String(value)).toISOString();
  return value;
}
