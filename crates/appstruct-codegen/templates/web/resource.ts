import { t } from "./ui";
import type {
  AggregateQuery,
  AggregateResponse,
  ApiError,
  BulkDeleteRequest,
  BulkResult,
  BulkUpdateRequest,
  CursorListQuery,
  CursorListResponse,
  CollectionBatch,
  CollectionResponse,
  ListQuery,
  ListResponse,
  FacetQuery,
  FacetResponse,
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
  if (resource.aggregateOwner && !["list", "read"].includes(operation))
    return false;
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
  collection?(
    id: string,
    name: string,
    options?: { signal?: AbortSignal },
  ): Promise<CollectionResponse>;
  saveCollection?(
    id: string,
    name: string,
    revision: number,
    input: CollectionBatch,
  ): Promise<CollectionResponse>;
  lookup?(
    ids: string[],
    options?: { signal?: AbortSignal },
  ): Promise<ResourceRecord[]>;
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
  facets?(
    query: FacetQuery,
    options?: { signal?: AbortSignal },
  ): Promise<FacetResponse>;
  get(id: string, options?: { signal?: AbortSignal }): Promise<ResourceRecord>;
  create(input: ResourceInput): Promise<ResourceRecord>;
  update(
    id: string,
    input: ResourceInput,
    options?: { revision?: number },
  ): Promise<ResourceRecord>;
  remove(id: string): Promise<void>;
  bulkUpdate(input: BulkUpdateRequest<ResourceInput>): Promise<BulkResult>;
  bulkDelete(input: BulkDeleteRequest): Promise<BulkResult>;
  exportCsv(query?: Omit<ListQuery, "page" | "page_size">): Promise<string>;
  downloadCsv?(query?: Omit<ListQuery, "page" | "page_size">): Promise<void>;
  importCsv(csv: string, batch: string): Promise<BulkResult>;
  importResult(batch: string): Promise<BulkResult>;
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

export type FieldSemantic = {
  kind: "money";
  currencyField: string;
  fractionDigits: number;
};

export interface FieldDefinition {
  name: string;
  apiName: string;
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
  requiredWhen?: { field: string; equals: string | boolean };
  greaterThan?: string;
  minLength?: number;
  maxLength?: number;
  minimum?: string;
  maximum?: string;
  uiComponent?: keyof AppStructRegistry["fields"];
  semantic?: FieldSemantic;
}

export function isSemanticCompanion(
  field: FieldDefinition,
  fields: FieldDefinition[],
): boolean {
  return fields.some(
    (candidate) =>
      candidate.semantic?.kind === "money" &&
      candidate.semantic.currencyField === field.name,
  );
}

/** Include readable fields required by cell formatting and row actions. */
export function listProjectionFields(
  resource: ResourceDefinition,
  visibleFields: FieldDefinition[],
  actor: AccessActor | null,
): string[] {
  const names = new Set([
    resource.primaryKey,
    "revision",
    resource.workflow?.field,
    ...visibleFields.map((field) => field.name),
  ]);
  function includeRule(rule?: AccessRule) {
    if (!rule) return;
    if (rule.mode === "any" || rule.mode === "all") {
      rule.rules.forEach(includeRule);
    } else if (rule.mode === "owner") {
      const logical = rule.field.split(".").at(-1) ?? rule.field;
      const field =
        resource.fields.find((field) => field.name === logical) ??
        resource.fields.find((field) => field.name === `${logical}_id`);
      if (field) names.add(field.name);
    }
  }
  [
    resource.access.read,
    resource.access.update,
    resource.access.delete,
  ].forEach(includeRule);
  for (const field of visibleFields) {
    if (field.semantic?.kind === "money")
      names.add(field.semantic.currencyField);
    includeRule(field.writeAccess);
  }
  return resource.fields
    .filter(
      (field) =>
        names.has(field.name) &&
        canAccessRule(field.readAccess ?? { mode: "public" }, actor),
    )
    .map((field) => field.name);
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
  aggregateOwner?: string;
  charts?: ChartDefinition[];
  collections?: CollectionDefinition[];
  displayField?: string;
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

export type ChartType = "kpi" | "bar" | "horizontal_bar" | "donut";

export interface ChartDefinition {
  name: string;
  label: string;
  type: ChartType;
  dimension?: string;
  measure: string;
  limit: number;
}

export interface CollectionDefinition {
  name: string;
  child: string;
  relation: string;
  maxItems: number;
  states: string[];
}

export function fieldErrors(error: unknown): Record<string, string> {
  const candidate = error as ApiError | undefined;
  return Object.fromEntries(
    candidate?.fields?.map((item) => [
      item.field,
      translatedMessage(item.message),
    ]) ?? [],
  );
}

function translatedMessage(message: string): string {
  for (const pattern of [
    /^(must contain at least )([0-9]+)( characters)$/,
    /^(must contain at most )([0-9]+)( characters)$/,
    /^(must be at least )(.+)$/,
    /^(must be at most )(.+)$/,
    /^(must be greater than )(.+)$/,
    /^(is required when )(.+)( matches its condition)$/,
  ]) {
    const match = pattern.exec(message);
    if (match) return t(`${match[1]}{0}${match[3] ?? ""}`, { 0: match[2] });
  }
  return t(message);
}

export function errorMessage(error: unknown): string {
  if (
    error instanceof TypeError &&
    /fetch|network|load failed/i.test(error.message)
  )
    return t("Network request failed");
  const message = (error as { message?: unknown } | null)?.message;
  return typeof message === "string"
    ? translatedMessage(message)
    : t("The request could not be completed");
}

/**
 * Start a browser download for `blob` under `filename`.
 *
 * The object URL is revoked on the next task rather than immediately: some browsers
 * begin the download asynchronously after `click()`, so revoking in the same tick can
 * abort it. A zero-delay timer keeps the URL alive for that handoff without leaking it.
 */
export function triggerDownload(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.rel = "noopener";
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}
