import { formatNumber, inputTimeZone, t } from "../ui";
import { uiText } from "../ui";
import { useQuery } from "@tanstack/react-query";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  RefreshCw,
  Save,
} from "lucide-react";
import { useMemo, type ComponentType } from "react";
import { AsyncState } from "../components/AsyncState";
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
import { errorMessage, isSemanticCompanion, useCanAccess } from "../resource";
import { useRelationOptions } from "../relation-options";

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
            <h1>{uiText("editRecord", { resource: resource.label })}</h1>
          </div>
        </div>
        <AsyncState state="loading" message={uiText("loadingRecord")} />
      </main>
    );
  }

  if (editing && canSubmit && recordQuery.error) {
    return (
      <main className="page form-page">
        <div className="page-heading">
          <div>
            <Link className="back-link" to={`/${resource.slug}`}>
              <ArrowLeft size={16} /> {resource.label}
            </Link>
            <h1>{uiText("editRecord", { resource: resource.label })}</h1>
          </div>
        </div>
        <AsyncState
          state="error"
          message={errorMessage(recordQuery.error)}
          onRetry={() => void recordQuery.refetch()}
        />
      </main>
    );
  }

  if (editing && canSubmit && !recordQuery.data) {
    return (
      <main className="page form-page">
        <div className="page-heading">
          <div>
            <Link className="back-link" to={`/${resource.slug}`}>
              <ArrowLeft size={16} /> {resource.label}
            </Link>
            <h1>{uiText("editRecord", { resource: resource.label })}</h1>
          </div>
        </div>
        <AsyncState state="empty" message={uiText("recordNotFound")} />
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
  const { form, fields, canSubmit, serverErrors, conflict, clearServerError } =
    controller;
  const renderedFields = useMemo(
    () => fields.filter((field) => !isSemanticCompanion(field, fields)),
    [fields],
  );
  if (!canSubmit) {
    return (
      <main className="page">
        <div className="alert" role="alert">
          {t("You do not have permission to change this resource.")}
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
            {uiText(editing ? "editRecord" : "addRecord", {
              resource: resource.label,
            })}
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
              disabled={controller.comparing}
              onClick={() => void controller.compareConflict()}
            >
              <RefreshCw size={16} /> {t("Compare changes")}
            </button>
          )}
        </div>
      )}
      {controller.comparisonError != null && (
        <div className="alert" role="alert">
          {errorMessage(controller.comparisonError)}
        </div>
      )}
      {controller.comparison && (
        <section aria-label={t("Resolve editing conflict")}>
          <p>
            {t(
              "Compare your draft with the latest record. Choose a value for each conflict, then review and save.",
            )}
          </p>
          <div className="table-frame conflict-comparison">
            <table>
              <thead>
                <tr>
                  <th> {t("Field")} </th>
                  <th>{t("Before editing")}</th>
                  <th>{t("Latest on server")}</th>
                  <th>{t("My changes")}</th>
                  <th>{t("Keep")}</th>
                </tr>
              </thead>
              <tbody>
                {controller.comparison.fields
                  .filter(
                    (field) =>
                      field.mine !== field.original ||
                      field.latest !== field.original,
                  )
                  .map((field) => (
                    <tr key={field.name}>
                      <th>{field.label}</th>
                      <td>{String(field.original)}</td>
                      <td>{String(field.latest)}</td>
                      <td>{String(field.mine)}</td>
                      <td>
                        <select
                          aria-label={t("Keep value for {0}", {
                            "0": field.label,
                          })}
                          value={field.choice ?? ""}
                          onChange={(event) =>
                            controller.chooseConflict(
                              field.name,
                              event.target.value as "mine" | "latest",
                            )
                          }
                        >
                          <option value="" disabled>
                            {t("Choose value")}
                          </option>
                          <option value="mine">{t("My changes")}</option>
                          <option value="latest">
                            {t("Latest on server")}
                          </option>
                        </select>
                      </td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>
          <button
            type="button"
            className="secondary-button"
            disabled={controller.comparison.fields.some(
              (field) => !field.choice,
            )}
            onClick={controller.applyConflict}
          >
            {t("Apply choices to draft")}
          </button>
        </section>
      )}
      {fields.some((field) => field.kind === "datetime") && (
        <p className="muted">
          {t("Times are entered in {0}.", { 0: inputTimeZone() })}
        </p>
      )}
      <form
        className="form-frame"
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          event.stopPropagation();
          const formElement = event.currentTarget;
          void (async () => {
            await form.handleSubmit();
            window.setTimeout(() => {
              formElement
                .querySelector<HTMLElement>('[aria-invalid="true"]')
                ?.focus();
            }, 0);
          })();
        }}
      >
        <form.Subscribe
          selector={(state) => [state.isDirty, state.isSubmitting] as const}
        >
          {([dirty, submitting]) => (
            <UnsavedChangesGuard enabled={dirty && !submitting} />
          )}
        </form.Subscribe>
        <fieldset
          className="form-grid"
          disabled={conflict}
          style={{ border: 0, padding: 0, margin: 0 }}
        >
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
        </fieldset>
        <div className="form-actions">
          <Link className="secondary-button" to={`/${resource.slug}`}>
            {uiText("cancel")}
          </Link>
          <form.Subscribe
            selector={(state) => [state.canSubmit, state.isSubmitting] as const}
          >
            {([ready, submitting]) => (
              <button
                className="primary-button"
                disabled={!ready || submitting || conflict}
              >
                <Save size={17} /> {uiText(submitting ? "saving" : "save")}
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
      title={t("Discard unsaved changes?")}
      description={t("Your changes on this form will be lost.")}
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
          {t("Money field renderer unavailable")}
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
            aria-label={t("{0} currency", { 0: field.label })}
            required={companion.field.required}
            value={String(companion.value ?? "")}
            onBlur={companion.onBlur}
            onChange={(event) => companion.onChange(event.target.value)}
            aria-invalid={Boolean(companion.error)}
            aria-describedby={errorMessage ? `${id}-error` : undefined}
          >
            <option value=""> {t("Select")} </option>
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
        <label htmlFor={id}>
          {field.label}
          {field.required && <span aria-hidden> *</span>}
        </label>
        {Component ? (
          <Component
            id={id}
            label={field.label}
            required={field.required}
            value={value}
            error={error}
            readOnly={false}
            onChange={onChange}
          />
        ) : (
          <div className="alert" role="alert">
            {t("Field renderer unavailable")}
          </div>
        )}
      </div>
    );
  }
  if (field.kind === "boolean" && !field.required)
    return (
      <div className="field">
        <label htmlFor={id}>{field.label}</label>
        <select
          id={id}
          value={value === "" ? "" : String(value)}
          onBlur={onBlur}
          onChange={(event) =>
            onChange(
              event.target.value === "" ? "" : event.target.value === "true",
            )
          }
          aria-invalid={Boolean(error)}
          aria-describedby={error ? `${id}-error` : undefined}
        >
          <option value="">{t("None")}</option>
          <option value="true">{t("Yes")}</option>
          <option value="false">{t("No")}</option>
        </select>
        {error && (
          <small id={`${id}-error`} className="field-error">
            {error}
          </small>
        )}
      </div>
    );
  if (field.kind === "boolean")
    return (
      <label className="checkbox-field">
        <input
          id={id}
          type="checkbox"
          checked={Boolean(value)}
          onBlur={onBlur}
          onChange={(event) => onChange(event.target.checked)}
          aria-invalid={Boolean(error)}
          aria-describedby={error ? `${id}-error` : undefined}
        />{" "}
        <span>{field.label}</span>
        {error && (
          <small id={`${id}-error`} className="field-error">
            {error}
          </small>
        )}
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
          <option value=""> {t("Select")} </option>
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
  const {
    options,
    pages,
    loadError,
    canLoad,
    isFetching,
    isPending,
    search,
    page,
    setSearch,
    setPage,
  } = useRelationOptions(target, value);
  const errorId = `${id}-error`;
  return (
    <div className="field">
      <label htmlFor={id}>
        {field.label}
        {field.required && <span aria-hidden> *</span>}
      </label>
      <label className="sr-only" htmlFor={`${id}-search`}>
        {t("Search")}
        {field.label}
      </label>
      <input
        id={`${id}-search`}
        value={search}
        placeholder={t("Search")}
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
        aria-busy={isFetching}
        disabled={isPending && canLoad}
      >
        <option value=""> {t("Select")} </option>
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
          onClick={() => setPage(page - 1)}
          aria-label={t("Previous options")}
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
          onClick={() => setPage(page + 1)}
          aria-label={t("Next options")}
        >
          <ChevronRight size={14} />
        </button>
      </span>
      <span className="sr-only" role="status" aria-live="polite">
        {isFetching
          ? t("Loading {0} options", { "0": field.label })
          : t("{0} {1} options loaded", {
              0: formatNumber(options.length),
              1: field.label,
            })}
      </span>
      {(error || loadError) && (
        <small id={errorId} className="field-error">
          {error || loadError}
        </small>
      )}
    </div>
  );
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
