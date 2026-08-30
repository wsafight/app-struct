import type { RowSelectionState } from "@tanstack/react-table";
import { Check, RotateCcw, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { AccessActor, FieldDefinition, ResourceDefinition, ResourceRecord } from "../../resource";
import { canAccessRule } from "../../resource";

interface BulkActionOptions {
  resource: ResourceDefinition;
  records: ResourceRecord[];
  rowSelection: RowSelectionState;
  trashMode: boolean;
  actor: AccessActor | null;
  runChange: (operation: () => Promise<void>) => Promise<boolean>;
  onError: (message: string) => void;
}

export function useBulkActions({ resource, records, rowSelection, trashMode, actor, runChange, onError }: BulkActionOptions) {
  const writableFields = useMemo(
    () => resource.fields.filter((field) => !field.readOnly && !field.primaryKey && canAccessRule(field.writeAccess ?? { mode: "public" }, actor)),
    [actor, resource],
  );
  const [bulkField, setBulkField] = useState(writableFields[0]?.name ?? "");
  const [bulkValue, setBulkValue] = useState("");
  const selectedIds = useMemo(() => Object.keys(rowSelection), [rowSelection]);

  useEffect(() => {
    if (!writableFields.some((field) => field.name === bulkField)) setBulkField(writableFields[0]?.name ?? "");
  }, [bulkField, writableFields]);

  function revisionMap(ids: string[]): Record<string, number> {
    return Object.fromEntries(ids.map((id) => [id, Number(records.find((record) => String(record[resource.primaryKey]) === id)?.revision ?? 0)]));
  }

  async function bulkDelete() {
    if (!selectedIds.length || !window.confirm(`${trashMode ? "Permanently delete" : resource.softDelete ? "Move to trash" : "Delete"} ${selectedIds.length} selected ${resource.label} records?`)) return;
    await runChange(async () => {
      const result = await resource.api.bulkDelete({ ids: selectedIds, expected_revisions: revisionMap(selectedIds) });
      if (result.failed.length) onError(`${result.failed.length} records could not be deleted`);
    });
  }

  async function restoreSelected() {
    if (!selectedIds.length || !resource.api.restore) return;
    await runChange(async () => {
      const result = await resource.api.restore!({ ids: selectedIds, expected_revisions: revisionMap(selectedIds) });
      if (result.failed.length) onError(`${result.failed.length} records could not be restored`);
    });
  }

  async function bulkUpdate() {
    const field = resource.fields.find((candidate) => candidate.name === bulkField);
    if (!field || !selectedIds.length) return;
    await runChange(async () => {
      const result = await resource.api.bulkUpdate({
        ids: selectedIds,
        patch: { [field.name]: inputValue(bulkValue, field) },
        expected_revisions: revisionMap(selectedIds),
      });
      if (result.failed.length) onError(`${result.failed.length} records could not be updated`);
    });
  }

  return { selectedIds, writableFields, bulkField, setBulkField, bulkValue, setBulkValue, bulkDelete, restoreSelected, bulkUpdate };
}

export function BulkToolbar({ actions, trashMode, busy }: { actions: ReturnType<typeof useBulkActions>; trashMode: boolean; busy: boolean }) {
  if (!actions.selectedIds.length) return null;
  return <div className="bulk-toolbar">
    <strong>{actions.selectedIds.length} selected</strong>
    {!trashMode && actions.writableFields.length > 0 && <>
      <select aria-label="Field to update" value={actions.bulkField} onChange={(event) => actions.setBulkField(event.target.value)}>
        {actions.writableFields.map((field) => <option key={field.name} value={field.name}>{field.label}</option>)}
      </select>
      <input aria-label="Bulk value" value={actions.bulkValue} onChange={(event) => actions.setBulkValue(event.target.value)} />
      <button className="secondary-button" disabled={busy} onClick={() => void actions.bulkUpdate()}><Check size={16} /> Apply</button>
    </>}
    {trashMode
      ? <button className="secondary-button" disabled={busy} onClick={() => void actions.restoreSelected()}><RotateCcw size={16} /> Restore</button>
      : <button className="icon-button danger" disabled={busy} onClick={() => void actions.bulkDelete()} title="Delete selected" aria-label="Delete selected"><Trash2 size={16} /></button>}
  </div>;
}

function inputValue(value: string, field: FieldDefinition): unknown {
  if (field.kind === "boolean") return value === "true";
  if (["integer", "bigint"].includes(field.kind)) return Number(value);
  return value;
}
