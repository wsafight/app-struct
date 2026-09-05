import { Columns3, RotateCcw } from "lucide-react";
import { isSemanticCompanion, type FieldDefinition } from "../../resource";

const DEFAULT_COLUMN_COUNT = 6;

export function defaultVisibleFieldNames(fields: FieldDefinition[]): string[] {
  return fields
    .filter((field) => !isSemanticCompanion(field, fields))
    .slice(0, DEFAULT_COLUMN_COUNT)
    .map((field) => field.name);
}

export function resolveVisibleFields(
  fields: FieldDefinition[],
  selection: string | null,
): FieldDefinition[] {
  if (!selection) {
    const defaults = new Set(defaultVisibleFieldNames(fields));
    return fields.filter((field) => defaults.has(field.name));
  }
  const selected = new Set(selection.split(","));
  const visible = fields.filter((field) => selected.has(field.name));
  if (visible.length > 0) return visible;
  const defaults = new Set(defaultVisibleFieldNames(fields));
  return fields.filter((field) => defaults.has(field.name));
}

export function ViewOptions({
  fields,
  visibleFieldNames,
  pageSize,
  onColumnsChange,
  onPageSizeChange,
}: {
  fields: FieldDefinition[];
  visibleFieldNames: string[];
  pageSize: number;
  onColumnsChange(names: string[]): void;
  onPageSizeChange(value: number): void;
}) {
  const selected = new Set(visibleFieldNames);
  const defaults = defaultVisibleFieldNames(fields);
  const customized = visibleFieldNames.join(",") !== defaults.join(",");

  function toggle(name: string, checked: boolean) {
    const next = checked
      ? [...selected, name]
      : visibleFieldNames.filter((field) => field !== name);
    onColumnsChange(
      fields
        .filter((field) => next.includes(field.name))
        .map((field) => field.name),
    );
  }

  return (
    <div className="view-options">
      <label>
        Rows
        <select
          aria-label="Rows per page"
          value={pageSize}
          onChange={(event) => onPageSizeChange(Number(event.target.value))}
        >
          {[25, 50, 100].map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </label>
      <details className="column-picker">
        <summary
          className="icon-button"
          title="Choose columns"
          aria-label="Choose columns"
        >
          <Columns3 size={17} />
        </summary>
        <div className="column-menu">
          {fields.map((field) => (
            <label key={field.name}>
              <input
                type="checkbox"
                checked={selected.has(field.name)}
                disabled={selected.has(field.name) && selected.size === 1}
                onChange={(event) => toggle(field.name, event.target.checked)}
              />
              <span>{field.label}</span>
            </label>
          ))}
          <div className="column-menu-footer">
            <button
              type="button"
              className="icon-button"
              title="Reset columns"
              aria-label="Reset columns"
              disabled={!customized}
              onClick={() => onColumnsChange(defaults)}
            >
              <RotateCcw size={15} />
            </button>
          </div>
        </div>
      </details>
    </div>
  );
}
