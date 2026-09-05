import type {
  AggregateQuery,
  AggregateResponse,
  ApiError,
  BulkDeleteRequest,
  BulkResult,
  BulkUpdateRequest,
  CursorListQuery,
  CursorListResponse,
  ListQuery,
  ListResponse,
  WorkflowCapabilities,
} from "./generated/client";
import type { AppStructRegistry } from "./generated/registry";
import {
  createContext,
  createElement,
  type ReactNode,
  useContext,
} from "react";

export type ResourceRecord = Record<string, unknown>;
export type ResourceInput = Record<string, unknown>;

export type AccessRule =
  | { mode: "public" }
  | { mode: "authenticated" }
  | { mode: "role"; role: string }
  | { mode: "owner"; field: string }
  | { mode: "any"; rules: AccessRule[] }
  | { mode: "all"; rules: AccessRule[] };

export type ResourceOperation =
  "list" | "read" | "create" | "update" | "delete";

export interface ResourceAccess {
  list: AccessRule;
  read: AccessRule;
  create: AccessRule;
  update: AccessRule;
  delete: AccessRule;
}

export interface AccessActor {
  id: string;
  roles: string[];
}

const ResourceActorContext = createContext<AccessActor | null>(null);

export function ResourceActorProvider({
  user,
  children,
}: {
  user: AccessActor | null;
  children: ReactNode;
}) {
  return createElement(
    ResourceActorContext.Provider,
    { value: user },
    children,
  );
}

export function useResourceActor(): AccessActor | null {
  return useContext(ResourceActorContext);
}

export function canAccessRule(
  rule: AccessRule,
  actor: AccessActor | null,
  record?: ResourceRecord,
): boolean {
  if (rule.mode === "public") return true;
  if (rule.mode === "authenticated") return actor !== null;
  if (rule.mode === "role") return actor?.roles.includes(rule.role) ?? false;
  if (rule.mode === "any")
    return rule.rules.some((item) => canAccessRule(item, actor, record));
  if (rule.mode === "all")
    return rule.rules.every((item) => canAccessRule(item, actor, record));
  if (!actor) return false;
  if (!record) return true;
  const logicalField = rule.field.split(".").at(-1) ?? rule.field;
  const field = logicalField in record ? logicalField : `${logicalField}_id`;
  return String(record[field] ?? "") === actor.id;
}

export function canAccessResource(
  resource: ResourceDefinition,
  operation: ResourceOperation,
  actor: AccessActor | null,
  record?: ResourceRecord,
): boolean {
  return canAccessRule(resource.access[operation], actor, record);
}

export function useCanAccess(
  resource: ResourceDefinition,
  operation: ResourceOperation,
  record?: ResourceRecord,
): boolean {
  return canAccessResource(resource, operation, useResourceActor(), record);
}

export function useCanAccessRule(rule: AccessRule): boolean {
  return canAccessRule(rule, useResourceActor());
}

export function useVisibleResources(
  resources: ResourceDefinition[],
): ResourceDefinition[] {
  const actor = useResourceActor();
  return resources.filter((resource) =>
    canAccessResource(resource, "list", actor),
  );
}

export interface ResourceApi {
  list(
    query?: ListQuery,
    options?: { signal?: AbortSignal },
  ): Promise<ListResponse<ResourceRecord>>;
  listCursor(
    query?: CursorListQuery,
    options?: { signal?: AbortSignal },
  ): Promise<CursorListResponse<ResourceRecord>>;
  aggregate(
    query?: AggregateQuery,
    options?: { signal?: AbortSignal },
  ): Promise<AggregateResponse>;
  get(id: string, options?: { signal?: AbortSignal }): Promise<ResourceRecord>;
  create(input: ResourceInput): Promise<ResourceRecord>;
  update(id: string, input: ResourceInput): Promise<ResourceRecord>;
  remove(id: string): Promise<void>;
  bulkUpdate(input: BulkUpdateRequest<ResourceInput>): Promise<BulkResult>;
  bulkDelete(input: BulkDeleteRequest): Promise<BulkResult>;
  exportCsv(): Promise<string>;
  importCsv(csv: string): Promise<BulkResult>;
  restore?(input: BulkDeleteRequest): Promise<BulkResult>;
  trash?(
    query?: Pick<ListQuery, "page" | "page_size">,
    options?: { signal?: AbortSignal },
  ): Promise<ListResponse<ResourceRecord>>;
  transitions?(
    id: string,
    options?: { signal?: AbortSignal },
  ): Promise<WorkflowCapabilities>;
  transition?(
    id: string,
    action: string,
    input?: unknown,
  ): Promise<ResourceRecord>;
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
  readAccess?: AccessRule;
  writeAccess?: AccessRule;
  values?: string[];
  relation?: string;
  minimum?: string;
  maximum?: string;
  uiComponent?: keyof AppStructRegistry["fields"];
}

export interface WorkflowInputFieldDefinition {
  name: string;
  label: string;
  kind: FieldKind;
  required: boolean;
  values?: string[];
  relation?: string;
}

export interface WorkflowTransitionDefinition {
  name: string;
  label: string;
  to: string;
  input?: {
    name: string;
    fields: WorkflowInputFieldDefinition[];
  };
}

export interface WorkflowDefinition {
  field: string;
  transitions: WorkflowTransitionDefinition[];
}

export interface ActivityDefinition {
  maxCommentBytes: number;
  attachments: boolean;
  adminRoles: string[];
}

export interface ResourceDefinition {
  id: string;
  name: string;
  eventPrefix: string;
  label: string;
  slug: string;
  primaryKey: string;
  softDelete: boolean;
  access: ResourceAccess;
  fields: FieldDefinition[];
  workflow?: WorkflowDefinition;
  activity?: ActivityDefinition;
  api: ResourceApi;
}

export function fieldErrors(error: unknown): Record<string, string> {
  const candidate = error as ApiError | undefined;
  return Object.fromEntries(
    candidate?.fields?.map((item) => [item.field, item.message]) ?? [],
  );
}

export function errorMessage(error: unknown): string {
  return error instanceof Error
    ? error.message
    : "The request could not be completed";
}
