import type { FieldDefinition } from "../../resource";
import { toApiValue } from "../../field-values";

export const MAX_IMPORT_BYTES = 2 * 1024 * 1024;
export const MAX_IMPORT_ROWS = 10_000;

export function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let value = "";
  let quoted = false;
  let closed = false;
  const input = text.replace(/^\uFEFF/, "");
  function endRow() {
    row.push(value);
    if (row.length > 1 || row[0] !== "") rows.push(row);
    row = [];
    value = "";
    closed = false;
    if (rows.length > MAX_IMPORT_ROWS + 1)
      throw new Error(`CSV import is limited to ${MAX_IMPORT_ROWS} rows.`);
  }
  for (let index = 0; index < input.length; index++) {
    const character = input[index];
    if (quoted) {
      if (character === '"') {
        if (input[index + 1] === '"') {
          value += '"';
          index++;
        } else {
          quoted = false;
          closed = true;
        }
      } else value += character;
    } else if (character === ",") {
      row.push(value);
      value = "";
      closed = false;
    } else if (character === "\n" || character === "\r") {
      if (character === "\r" && input[index + 1] === "\n") index++;
      endRow();
    } else if (character === '"' && !value && !closed) {
      quoted = true;
    } else if (character === '"' || closed) {
      throw new Error("CSV contains an invalid quoted field.");
    } else value += character;
  }
  if (quoted) throw new Error("CSV contains an unterminated quoted field.");
  if (value || row.length || closed) endRow();
  if (!rows.length) throw new Error("CSV is empty.");
  if (new Set(rows[0]).size !== rows[0].length || rows[0].some((name) => !name))
    throw new Error("CSV headers must be nonempty and unique.");
  return rows;
}

export function writeCsv(rows: string[][]): string {
  return (
    rows
      .map((row) =>
        row
          .map((value) =>
            /[",\r\n]/.test(value) ? `"${value.replaceAll('"', '""')}"` : value,
          )
          .join(","),
      )
      .join("\r\n") + "\r\n"
  );
}

export function csvValueError(
  value: string,
  field: FieldDefinition,
): string | undefined {
  try {
    if (field.kind === "boolean" && value && !["true", "false"].includes(value))
      return `${field.label} must be true or false`;
    if (field.kind === "datetime" && /(?:Z|[+-]\d{2}:\d{2})$/.test(value)) {
      if (Number.isNaN(Date.parse(value)))
        return `${field.label} must be a valid date and time`;
      return undefined;
    }
    toApiValue(
      field.kind === "boolean" && value ? value === "true" : value,
      field,
    );
    return undefined;
  } catch (error) {
    return error instanceof Error ? error.message : "Invalid value";
  }
}
