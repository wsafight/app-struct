import { t } from "../../ui";
import type { RowSelectionState } from "@tanstack/react-table";
import { Check, RotateCcw, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { DialogFrame } from "../../components/Dialog";
import { toApiValue, valueError, type FormValue } from "../../field-values";
import type { BulkResult } from "../../generated/client";
import { FieldControl } from "../ResourceForm";
import type {
  AccessActor,
  ResourceDefinition,
  ResourceInput,
  ResourceRecord,
} from "../../resource";
import { canAccessResource, canAccessRule, errorMessage } from "../../resource";
import { revisionMap } from "./revisions";

interface BulkActionOptions {
  resource: ResourceDefinition;
  records: ResourceRecord[];
  rowSelection: RowSelectionState;
  trashMode: boolean;
  actor: AccessActor | null;
  runChange: (operation: () => Promise<void>) => Promise<boolean>;
  onError: (message: string) => void;
}
interface Operation {
  kind: "update" | "delete" | "restore";
  ids: string[];
  patch?: ResourceInput;
  revisions: Record<string, number>;
}
interface Outcome {
  operation: Operation;
  result: BulkResult;
}

export function useBulkActions({
  resource,
  records,
  rowSelection,
  trashMode,
  actor,
  runChange,
  onError,
}: BulkActionOptions) {
  const writableFields = useMemo(
    () =>
      resource.fields.filter(
        (field) =>
          !resource.aggregateOwner &&
          !field.readOnly &&
          !field.primaryKey &&
          !field.uiComponent &&
          canAccessRule(field.writeAccess ?? { mode: "public" }, actor) &&
          (!field.semantic ||
            resource.fields.some(
              (companion) =>
                companion.name === field.semantic?.currencyField &&
                !companion.readOnly &&
                canAccessRule(
                  companion.writeAccess ?? { mode: "public" },
                  actor,
                ),
            )),
      ),
    [actor, resource],
  );
  const [bulkField, setBulkField] = useState(writableFields[0]?.name ?? "");
  const [bulkValue, setBulkValue] = useState<FormValue>(
    writableFields[0]?.kind === "boolean" ? false : "",
  );
  const [currency, setCurrency] = useState<FormValue>("");
  const [preview, setPreview] = useState<Operation | null>(null);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [working, setWorking] = useState(false);
  const selectedIds = useMemo(
    () =>
      Object.entries(rowSelection)
        .filter(([, selected]) => selected)
        .map(([id]) => id),
    [rowSelection],
  );
  const field = writableFields.find((field) => field.name === bulkField);
  const companion = field?.semantic
    ? resource.fields.find(
        (candidate) => candidate.name === field.semantic?.currencyField,
      )
    : undefined;
  const validationError = field ? valueError(bulkValue, field) : undefined;
  const currencyError = companion
    ? (valueError(currency, companion) ??
      (Boolean(bulkValue) !== Boolean(currency)
        ? t("Set or clear both the amount and currency.")
        : undefined))
    : undefined;
  const canUpdate = selectedIds.some((id) =>
    canAccessResource(
      resource,
      "update",
      actor,
      records.find((record) => String(record[resource.primaryKey]) === id),
    ),
  );
  const canDelete = selectedIds.some((id) =>
    canAccessResource(
      resource,
      "delete",
      actor,
      records.find((record) => String(record[resource.primaryKey]) === id),
    ),
  );

  useEffect(() => {
    if (!writableFields.some((field) => field.name === bulkField)) {
      setBulkField(writableFields[0]?.name ?? "");
      setBulkValue(writableFields[0]?.kind === "boolean" ? false : "");
      setCurrency("");
    }
  }, [bulkField, writableFields]);

  function prepare(kind: Operation["kind"]) {
    if (!selectedIds.length || working) return;
    try {
      const patch: ResourceInput = {};
      if (kind === "update") {
        if (!field) return;
        patch[field.apiName] = toApiValue(bulkValue, field);
        if (companion)
          patch[companion.apiName] = toApiValue(currency, companion);
      }
      setPreview({
        kind,
        ids: [...selectedIds],
        revisions: revisionMap(records, resource.primaryKey, selectedIds),
        ...(kind === "update" ? { patch } : {}),
      });
      setOutcome(null);
    } catch (error) {
      onError(errorMessage(error));
    }
  }

  async function execute() {
    if (!preview || working) return;
    const operation = preview;
    setWorking(true);
    try {
      let result: BulkResult | undefined;
      const changed = await runChange(async () => {
        const input = {
          ids: operation.ids,
          expected_revisions: operation.revisions,
        };
        if (operation.kind === "update")
          result = await resource.api.bulkUpdate({
            ...input,
            patch: operation.patch!,
          });
        else if (operation.kind === "restore")
          result = await resource.api.restore!(input);
        else result = await resource.api.bulkDelete(input);
      });
      if (changed && result) {
        setOutcome({ operation, result });
        setPreview(null);
      }
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setWorking(false);
    }
  }

  async function reviewRetry() {
    if (!outcome || working) return;
    const ids = outcome.result.failed.map((failure) => failure.id);
    setWorking(true);
    try {
      // Refresh conflicts before asking the user to review a second write.
      const latest =
        outcome.operation.kind === "restore"
          ? records.filter((record) =>
              ids.includes(String(record[resource.primaryKey])),
            )
          : resource.api.lookup
            ? await resource.api.lookup(ids)
            : await Promise.all(ids.map((id) => resource.api.get(id)));
      if (latest.length !== ids.length)
        throw new Error(
          t(
            "Some failed records are no longer accessible. Refresh the list before retrying.",
          ),
        );
      setPreview({
        ...outcome.operation,
        ids,
        revisions: revisionMap(latest, resource.primaryKey, ids),
      });
      setOutcome(null);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setWorking(false);
    }
  }

  return {
    selectedIds,
    writableFields,
    field,
    companion,
    bulkField,
    bulkValue,
    currency,
    validationError,
    currencyError,
    canUpdate,
    canDelete,
    preview,
    outcome,
    working,
    execute,
    reviewRetry,
    close: () => {
      if (!working) {
        setPreview(null);
        setOutcome(null);
      }
    },
    setBulkField: (name: string) => {
      setBulkField(name);
      setBulkValue(
        writableFields.find((field) => field.name === name)?.kind === "boolean"
          ? false
          : "",
      );
      setCurrency("");
    },
    setBulkValue,
    setCurrency,
    bulkDelete: () => prepare("delete"),
    restoreSelected: () => {
      if (resource.api.restore) prepare("restore");
    },
    bulkUpdate: () => prepare("update"),
    actionLabel: (kind: Operation["kind"]) =>
      kind === "update"
        ? t("Update")
        : kind === "restore"
          ? t("Restore")
          : trashMode
            ? t("Permanently delete")
            : resource.softDelete
              ? t("Move to trash")
              : t("Delete"),
  };
}

export function BulkToolbar({
  actions,
  trashMode,
  busy,
  resources,
}: {
  actions: ReturnType<typeof useBulkActions>;
  trashMode: boolean;
  busy: boolean;
  resources: ResourceDefinition[];
}) {
  const disabled = busy || actions.working;
  return (
    <>
      {actions.selectedIds.length > 0 && (
        <div className="bulk-toolbar">
          <strong>
            {actions.selectedIds.length} {t("selected on this page")}
          </strong>
          {!trashMode &&
            actions.canUpdate &&
            actions.writableFields.length > 0 && (
              <>
                <select
                  aria-label={t("Field to update")}
                  value={actions.bulkField}
                  disabled={disabled}
                  onChange={(event) => actions.setBulkField(event.target.value)}
                >
                  {actions.writableFields.map((field) => (
                    <option key={field.name} value={field.name}>
                      {field.label}
                    </option>
                  ))}
                </select>
                {actions.field && (
                  <fieldset className="bulk-value" disabled={disabled}>
                    <FieldControl
                      idPrefix="bulk-"
                      field={actions.field}
                      resources={resources}
                      value={actions.bulkValue}
                      error={actions.validationError}
                      onBlur={() => undefined}
                      onChange={actions.setBulkValue}
                      companion={
                        actions.companion
                          ? {
                              field: actions.companion,
                              value: actions.currency,
                              error: actions.currencyError,
                              onBlur: () => undefined,
                              onChange: actions.setCurrency,
                            }
                          : undefined
                      }
                    />
                  </fieldset>
                )}
                <button
                  type="button"
                  className="secondary-button"
                  disabled={
                    disabled ||
                    Boolean(actions.validationError || actions.currencyError)
                  }
                  onClick={actions.bulkUpdate}
                >
                  <Check size={16} /> {t("Review update")}
                </button>
              </>
            )}
          {trashMode
            ? actions.canUpdate && (
                <button
                  type="button"
                  className="secondary-button"
                  disabled={disabled}
                  onClick={actions.restoreSelected}
                >
                  <RotateCcw size={16} /> {t("Restore")}
                </button>
              )
            : actions.canDelete && (
                <button
                  type="button"
                  className="icon-button danger"
                  disabled={disabled}
                  onClick={actions.bulkDelete}
                  title={t("Delete selected")}
                  aria-label={t("Delete selected")}
                >
                  <Trash2 size={16} />
                </button>
              )}
        </div>
      )}
      <DialogFrame
        open={Boolean(actions.preview || actions.outcome)}
        title={
          actions.preview
            ? t("Review bulk operation")
            : t("Bulk operation results")
        }
        onCancel={actions.close}
      >
        {actions.preview && (
          <>
            <p>
              {actions.actionLabel(actions.preview.kind)}{" "}
              {actions.preview.ids.length} {t("records.")}
            </p>
            {actions.preview.patch && (
              <dl className="bulk-patch">
                {Object.entries(actions.preview.patch).map(([name, value]) => (
                  <div key={name}>
                    <dt>{name}</dt>
                    <dd>
                      {value === null
                        ? t("Clear value")
                        : typeof value === "object"
                          ? JSON.stringify(value)
                          : String(value)}
                    </dd>
                  </div>
                ))}
              </dl>
            )}
            <details>
              <summary> {t("Selected records and revisions")}</summary>
              <ul>
                {actions.preview.ids.map((id) => (
                  <li key={id}>
                    {id} {t("· revision")}
                    {actions.preview!.revisions[id]}
                  </li>
                ))}
              </ul>
            </details>
            <div className="dialog-actions">
              <button
                type="button"
                className="secondary-button"
                disabled={disabled}
                onClick={actions.close}
              >
                {" "}
                {t("Cancel")}
              </button>
              <button
                type="button"
                className={
                  actions.preview.kind === "delete"
                    ? "danger-button"
                    : "primary-button"
                }
                disabled={disabled}
                onClick={() => void actions.execute()}
              >
                {disabled ? "Working..." : t("Confirm operation")}
              </button>
            </div>
          </>
        )}
        {actions.outcome && (
          <>
            <p role="status">
              {actions.outcome.result.succeeded.length} {t("succeeded ·")}{" "}
              {actions.outcome.result.failed.length} {t("failed")}
            </p>
            {actions.outcome.result.failed.length > 0 && (
              <div className="table-frame">
                <table>
                  <thead>
                    <tr>
                      <th> {t("Record")} </th>
                      <th>{t("Reason")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {actions.outcome.result.failed.map((failure) => (
                      <tr key={failure.id}>
                        <td>{failure.id}</td>
                        <td>
                          {errorMessage(failure)} ({failure.code})
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            <div className="dialog-actions">
              <button
                type="button"
                className="secondary-button"
                disabled={disabled}
                onClick={actions.close}
              >
                {t("Close")}
              </button>
              {actions.outcome.result.failed.length > 0 && (
                <button
                  type="button"
                  className="primary-button"
                  disabled={disabled}
                  onClick={() => void actions.reviewRetry()}
                >
                  {t("Review retry for failed records")}
                </button>
              )}
            </div>
          </>
        )}
      </DialogFrame>
    </>
  );
}
