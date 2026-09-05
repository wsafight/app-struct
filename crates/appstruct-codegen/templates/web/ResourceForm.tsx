import { useQuery } from "@tanstack/react-query";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  RefreshCw,
  Save,
} from "lucide-react";
import { useEffect, useMemo, useState, type ComponentType } from "react";
import { useResourceFormController } from "../controller";
import { inputType, type FormValue } from "../field-values";
import { ConfirmDialog } from "../components/Dialog";
import type {
  AppStructRegistry,
  FieldComponentProps,
} from "../generated/registry";
import { Link, useNavigate, useParams, useUnsavedChanges } from "../navigation";
import { resourceQueryKeys } from "../query";
import { recordLabel } from "../relations";
import type {
  FieldDefinition,
  ResourceDefinition,
  ResourceRecord,
} from "../resource";
import {
  canAccessResource,
  errorMessage,
  isSemanticCompanion,
  useCanAccess,
  useResourceActor,
} from "../resource";

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
  const editing = id !== undefined;
  const controller = useResourceFormController(resource, {
    id,
    initialRecord,
    refetchRecord,
    onSaved: async () => {
      await navigate(
        id
          ? `/${resource.slug}/${encodeURIComponent(id)}`
          : `/${resource.slug}`,
        { replace: true },
      );
    },
  });
  const {
    form,
    fields,
    canSubmit,
    serverErrors,
    conflict,
    reloadRecord,
    clearServerError,
  } = controller;
  const renderedFields = useMemo(
    () => fields.filter((field) => !isSemanticCompanion(field, fields)),
    [fields],
  );
  if (!canSubmit) {
    return (
      <main className="page">
        <div className="alert" role="alert">
          You do not have permission to change this resource.
        </div>
      </main>
    );
  }

  const pageError = controller.error
    ? errorMessage(controller.error)
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
          {renderedFields.map((field) => {
            const companion = field.semantic
              ? fields.find(
                  (candidate) =>
                    candidate.name === field.semantic?.currencyField,
                )
              : undefined;
            return (
              <form.Field key={field.name} name={field.name}>
                {(formField) =>
                  companion ? (
                    <form.Field name={companion.name}>
                      {(companionFormField) => (
                        <FieldControl
                          field={field}
                          resources={resources}
                          registry={registry}
                          value={formField.state.value}
                          error={
                            serverErrors[field.name] ??
                            validationMessage(formField.state.meta.errors)
                          }
                          companion={{
                            field: companion,
                            value: companionFormField.state.value,
                            error:
                              serverErrors[companion.name] ??
                              validationMessage(
                                companionFormField.state.meta.errors,
                              ),
                            onBlur: companionFormField.handleBlur,
                            onChange: (value) => {
                              clearServerError(companion.name);
                              companionFormField.handleChange(value);
                            },
                          }}
                          onBlur={formField.handleBlur}
                          onChange={(value) => {
                            clearServerError(field.name);
                            formField.handleChange(value);
                          }}
                        />
                      )}
                    </form.Field>
                  ) : (
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
                  )
                }
              </form.Field>
            );
          })}
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

export function UnsavedChangesGuard({ enabled }: { enabled: boolean }) {
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

export function FieldControl({
  idPrefix = "",
  field,
  resources,
  registry,
  value,
  error,
  companion,
  onBlur,
  onChange,
}: {
  idPrefix?: string;
  field: FieldDefinition;
  resources: ResourceDefinition[];
  registry?: AppStructRegistry;
  value: FormValue;
  error?: string;
  companion?: {
    field: FieldDefinition;
    value: FormValue;
    error?: string;
    onBlur(): void;
    onChange(value: FormValue): void;
  };
  onBlur(): void;
  onChange(value: FormValue): void;
}) {
  const id = `${idPrefix}field-${field.name}`;
  if (field.semantic?.kind === "money") {
    if (!companion) {
      return (
        <div className="alert" role="alert">
          Money field renderer unavailable
        </div>
      );
    }
    const errorMessage = error ?? companion.error;
    const step =
      field.semantic.fractionDigits === 0
        ? "1"
        : (1 / 10 ** field.semantic.fractionDigits).toFixed(
            field.semantic.fractionDigits,
          );
    return (
      <div className="field">
        <label htmlFor={id}>
          {field.label}
          {field.required && <span aria-hidden> *</span>}
        </label>
        <div className="money-control">
          <select
            id={`${id}-currency`}
            aria-label={`${field.label} currency`}
            required={companion.field.required}
            value={String(companion.value ?? "")}
            onBlur={companion.onBlur}
            onChange={(event) => companion.onChange(event.target.value)}
            aria-invalid={Boolean(companion.error)}
            aria-describedby={errorMessage ? `${id}-error` : undefined}
          >
            <option value="">Select</option>
            {companion.field.values?.map((item) => (
              <option key={item}>{item}</option>
            ))}
          </select>
          <input
            id={id}
            name={field.name}
            type="number"
            inputMode="decimal"
            required={field.required}
            value={String(value ?? "")}
            min={field.minimum}
            max={field.maximum}
            step={step}
            onBlur={onBlur}
            onChange={(event) => onChange(event.target.value)}
            aria-invalid={Boolean(error)}
            aria-describedby={errorMessage ? `${id}-error` : undefined}
          />
        </div>
        {errorMessage && (
          <small id={`${id}-error`} className="field-error">
            {errorMessage}
          </small>
        )}
      </div>
    );
  }
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
          inputMode={
            field.kind === "bigint"
              ? "numeric"
              : field.kind === "decimal"
                ? "decimal"
                : undefined
          }
          min={field.minimum}
          max={field.maximum}
          step={
            field.kind === "datetime" || field.kind === "decimal"
              ? "any"
              : undefined
          }
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
              {target ? recordLabel(target, record) : optionValue}
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
