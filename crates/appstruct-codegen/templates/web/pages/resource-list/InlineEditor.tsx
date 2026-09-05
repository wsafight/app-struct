import { Check, Pencil, X } from "lucide-react";
import { useState } from "react";
import {
  inputType,
  toApiValue,
  toFormValue,
  valueError,
} from "../../field-values";
import type { FieldDefinition, ResourceRecord } from "../../resource";

type EditorValue = string | boolean;

export function supportsInlineEdit(field: FieldDefinition): boolean {
  return (
    !field.primaryKey &&
    !field.readOnly &&
    !field.uiComponent &&
    !field.semantic &&
    [
      "string",
      "integer",
      "bigint",
      "decimal",
      "boolean",
      "date",
      "datetime",
      "enum",
    ].includes(field.kind)
  );
}

export function InlineEditor({
  record,
  field,
  onSave,
}: {
  record: ResourceRecord;
  field: FieldDefinition;
  onSave(value: unknown): Promise<boolean>;
}) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState<EditorValue>(() =>
    toFormValue(record[field.name], field),
  );
  const [saving, setSaving] = useState(false);

  function begin() {
    setValue(toFormValue(record[field.name], field));
    setEditing(true);
  }

  async function save() {
    if (valueError(value, field)) return;
    setSaving(true);
    try {
      if (await onSave(toApiValue(value, field, record[field.name])))
        setEditing(false);
    } finally {
      setSaving(false);
    }
  }

  if (!editing) {
    return (
      <div className="inline-value">
        <span>{displayValue(record[field.name])}</span>
        <button
          type="button"
          className="inline-edit-button"
          onClick={begin}
          title={`Edit ${field.label}`}
          aria-label={`Edit ${field.label}`}
        >
          <Pencil size={13} />
        </button>
      </div>
    );
  }
  return (
    <div className="inline-editor">
      {field.kind === "boolean" ? (
        <input
          type="checkbox"
          checked={Boolean(value)}
          onChange={(event) => setValue(event.target.checked)}
          aria-label={field.label}
        />
      ) : field.kind === "enum" ? (
        <select
          value={String(value)}
          onChange={(event) => setValue(event.target.value)}
          aria-label={field.label}
        >
          {!field.required && <option value="">None</option>}
          {field.values?.map((option) => (
            <option key={option}>{option}</option>
          ))}
        </select>
      ) : (
        <input
          type={inputType(field.kind)}
          inputMode={
            field.kind === "bigint"
              ? "numeric"
              : field.kind === "decimal"
                ? "decimal"
                : undefined
          }
          value={String(value)}
          min={field.minimum}
          max={field.maximum}
          step={
            field.kind === "datetime" || field.kind === "decimal"
              ? "any"
              : undefined
          }
          onChange={(event) => setValue(event.target.value)}
          aria-label={field.label}
        />
      )}
      <button
        type="button"
        className="inline-edit-button"
        disabled={saving || Boolean(valueError(value, field))}
        onClick={() => void save()}
        title="Save"
        aria-label={`Save ${field.label}`}
      >
        <Check size={14} />
      </button>
      <button
        type="button"
        className="inline-edit-button"
        disabled={saving}
        onClick={() => setEditing(false)}
        title="Cancel"
        aria-label={`Cancel ${field.label}`}
      >
        <X size={14} />
      </button>
    </div>
  );
}

function displayValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  return String(value);
}
