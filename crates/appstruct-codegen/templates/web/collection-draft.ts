import type { CollectionBatch } from "./generated/client";
import {
  toApiValue,
  toFormValue,
  valueError,
  type FormValues,
} from "./field-values";
import type {
  FieldDefinition,
  ResourceDefinition,
  ResourceRecord,
} from "./resource";

export interface CollectionRow {
  key: string;
  record?: ResourceRecord;
  values: FormValues;
}

export function collectionRow(
  fields: FieldDefinition[],
  resource: ResourceDefinition,
  record?: ResourceRecord,
): CollectionRow {
  return {
    key: record ? String(record[resource.primaryKey]) : crypto.randomUUID(),
    record,
    values: Object.fromEntries(
      fields.map((field) => [
        field.name,
        toFormValue(record?.[field.name], field),
      ]),
    ),
  };
}

export function collectionBatch(
  rows: CollectionRow[],
  original: ResourceRecord[],
  resource: ResourceDefinition,
  fields: FieldDefinition[],
) {
  const batch: Required<CollectionBatch> = {
    creates: [],
    updates: [],
    deletes: [],
  };
  const errors: Record<string, string> = {};
  for (const row of rows) {
    const input: Record<string, unknown> = {};
    for (const field of fields) {
      const value = row.values[field.name];
      if (row.record && value === toFormValue(row.record[field.name], field))
        continue;
      const error = valueError(value, field);
      if (error) errors[`${row.key}.${field.name}`] = error;
      else
        input[field.name] = toApiValue(value, field, row.record?.[field.name]);
    }
    if (row.record) {
      if (Object.keys(input).length)
        batch.updates.push({
          id: row.key,
          revision: Number(row.record.revision),
          input,
        });
    } else batch.creates.push({ key: row.key, input });
  }
  for (const record of original) {
    const id = String(record[resource.primaryKey]);
    if (!rows.some((row) => row.key === id))
      batch.deletes.push({ id, revision: Number(record.revision) });
  }
  return {
    batch,
    errors,
    dirty:
      Boolean(
        batch.creates.length + batch.updates.length + batch.deletes.length,
      ) || Object.keys(errors).length > 0,
  };
}
