import { Check, Pencil, X } from "lucide-react";
import { useState } from "react";
import type { FieldDefinition, ResourceRecord } from "../../resource";

type EditorValue = string | boolean;

export function supportsInlineEdit(field: FieldDefinition): boolean {
  return !field.primaryKey
    && !field.readOnly
    && !field.uiComponent
    && ["string", "integer", "bigint", "decimal", "boolean", "date", "datetime", "enum"].includes(field.kind);
}

export function InlineEditor({ record, field, onSave }: { record: ResourceRecord; field: FieldDefinition; onSave(value: unknown): Promise<boolean> }) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState<EditorValue>(() => formValue(record[field.name], field));
  const [saving, setSaving] = useState(false);

  function begin() {
    setValue(formValue(record[field.name], field));
    setEditing(true);
  }

  async function save() {
    if (!isValid(value, field)) return;
    setSaving(true);
    try {
      if (await onSave(apiValue(value, field))) setEditing(false);
    } finally {
      setSaving(false);
    }
  }

  if (!editing) {
    return <div className="inline-value"><span>{displayValue(record[field.name])}</span><button type="button" className="inline-edit-button" onClick={begin} title={`Edit ${field.label}`} aria-label={`Edit ${field.label}`}><Pencil size={13} /></button></div>;
  }
  return <div className="inline-editor">
    {field.kind === "boolean"
      ? <input type="checkbox" checked={Boolean(value)} onChange={(event) => setValue(event.target.checked)} aria-label={field.label} />
      : field.kind === "enum"
        ? <select value={String(value)} onChange={(event) => setValue(event.target.value)} aria-label={field.label}>{!field.required && <option value="">None</option>}{field.values?.map((option) => <option key={option}>{option}</option>)}</select>
        : <input type={inputType(field.kind)} value={String(value)} min={field.minimum} max={field.maximum} step={field.kind === "decimal" ? "any" : undefined} onChange={(event) => setValue(event.target.value)} aria-label={field.label} />}
    <button type="button" className="inline-edit-button" disabled={saving || !isValid(value, field)} onClick={() => void save()} title="Save" aria-label={`Save ${field.label}`}><Check size={14} /></button>
    <button type="button" className="inline-edit-button" disabled={saving} onClick={() => setEditing(false)} title="Cancel" aria-label={`Cancel ${field.label}`}><X size={14} /></button>
  </div>;
}

function formValue(value: unknown, field: FieldDefinition): EditorValue {
  if (field.kind === "boolean") return Boolean(value);
  if (field.kind === "datetime" && typeof value === "string") return value.slice(0, 16);
  return value == null ? "" : String(value);
}

function apiValue(value: EditorValue, field: FieldDefinition): unknown {
  if (value === "") return field.required ? value : null;
  if (["integer", "bigint", "decimal"].includes(field.kind)) return Number(value);
  if (field.kind === "datetime") return new Date(String(value)).toISOString();
  return value;
}

function isValid(value: EditorValue, field: FieldDefinition): boolean {
  if (value === "") return !field.required;
  if (["integer", "bigint", "decimal"].includes(field.kind)) return Number.isFinite(Number(value));
  if (field.kind === "datetime" || field.kind === "date") return !Number.isNaN(Date.parse(String(value)));
  return true;
}

function inputType(kind: FieldDefinition["kind"]): string {
  if (["integer", "bigint", "decimal"].includes(kind)) return "number";
  if (kind === "date") return "date";
  if (kind === "datetime") return "datetime-local";
  return "text";
}

function displayValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  return String(value);
}
