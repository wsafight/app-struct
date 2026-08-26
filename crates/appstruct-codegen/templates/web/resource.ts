import type { ApiError } from "./generated/client";

export type ResourceRecord = Record<string, unknown>;
export type ResourceInput = Record<string, unknown>;

export interface ResourceApi {
  list(): Promise<ResourceRecord[]>;
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
  values?: string[];
  relation?: string;
}

export interface ResourceDefinition {
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

