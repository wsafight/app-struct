import type { ApiError, ListQuery, ListResponse } from "./generated/client";
import type { AppStructRegistry } from "./generated/registry";

export type ResourceRecord = Record<string, unknown>;
export type ResourceInput = Record<string, unknown>;

export interface ResourceApi {
  list(query?: ListQuery): Promise<ListResponse<ResourceRecord>>;
  get(id: string): Promise<ResourceRecord>;
  create(input: ResourceInput): Promise<ResourceRecord>;
  update(id: string, input: ResourceInput): Promise<ResourceRecord>;
  remove(id: string): Promise<void>;
}

export type FieldKind =
  | "uuid"
  | "string"
  | "text"
  | "integer"
  | "bigint"
  | "decimal"
  | "boolean"
  | "date"
  | "datetime"
  | "json"
  | "enum"
  | "relation";

export interface FieldDefinition {
  name: string;
  label: string;
  kind: FieldKind;
  required: boolean;
  readOnly: boolean;
  primaryKey: boolean;
  searchable: boolean;
  filterable: boolean;
  sortable: boolean;
  values?: string[];
  relation?: string;
  minimum?: string;
  maximum?: string;
  uiComponent?: keyof AppStructRegistry["fields"];
}

export interface ResourceDefinition {
  id: string;
  name: string;
  label: string;
  slug: string;
  primaryKey: string;
  fields: FieldDefinition[];
  api: ResourceApi;
}

export function fieldErrors(error: unknown): Record<string, string> {
  const candidate = error as ApiError | undefined;
  return Object.fromEntries(candidate?.fields?.map((item) => [item.field, item.message]) ?? []);
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "The request could not be completed";
}
