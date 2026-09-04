import { describe, expect, it } from "vitest";
import { ApiError, requestHeaders, tenantApi } from "./generated/client";
import { validateResourceSearch } from "./navigation";
import {
  buildResourceFilterQuery,
  supportsRange,
} from "./pages/ResourceFilters";
import { supportsInlineEdit } from "./pages/resource-list/InlineEditor";
import { formatValue } from "./pages/resource-list/ResourceTable";
import { appQueryKeys, resourceQueryKeys, shouldRetryQuery } from "./query";
import {
  canAccessResource,
  canAccessRule,
  errorMessage,
  fieldErrors,
  type FieldDefinition,
  type ResourceDefinition,
} from "./resource";

describe("validateResourceSearch", () => {
  it("normalizes supported resource search parameters", () => {
    expect(
      validateResourceSearch({
        page: "10000",
        page_size: "50",
        sort: "-created_at",
        q: "quarterly report",
        trash: 1,
        "filter[status]": "open",
        "filter[created_at][gte]": "2026-01-01",
      }),
    ).toEqual({
      page: 10000,
      page_size: 50,
      sort: "-created_at",
      q: "quarterly report",
      trash: "1",
      "filter[status]": "open",
      "filter[created_at][gte]": "2026-01-01",
    });
  });

  it("omits defaults and rejects unsupported values", () => {
    expect(
      validateResourceSearch({
        page: "1",
        page_size: 25,
        sort: [],
        q: "",
        trash: "true",
        unknown: "value",
        "filter[status]": "",
        "filter[]": "value",
        "filter[broken": "value",
        "filter[created_at][unknown]": "value",
      }),
    ).toEqual({});
  });

  it("rejects invalid pagination bounds", () => {
    expect(validateResourceSearch({ page: "0", page_size: "101" })).toEqual({});
    expect(validateResourceSearch({ page: "10001", page_size: "25" })).toEqual(
      {},
    );
    expect(
      validateResourceSearch({ page: "1.5", page_size: Number.NaN }),
    ).toEqual({});
  });
});

describe("shouldRetryQuery", () => {
  it("retries transient HTTP and network failures", () => {
    expect(
      shouldRetryQuery(0, new ApiError(429, "RATE_LIMITED", "slow down")),
    ).toBe(true);
    expect(
      shouldRetryQuery(1, new ApiError(503, "UNAVAILABLE", "try again")),
    ).toBe(true);
    expect(shouldRetryQuery(0, new TypeError("network unavailable"))).toBe(
      true,
    );
  });

  it("does not retry permanent, exhausted, or programming failures", () => {
    expect(
      shouldRetryQuery(0, new ApiError(400, "BAD_REQUEST", "invalid")),
    ).toBe(false);
    expect(
      shouldRetryQuery(2, new ApiError(503, "UNAVAILABLE", "try again")),
    ).toBe(false);
    expect(shouldRetryQuery(0, new Error("unexpected"))).toBe(false);
  });
});

describe("requestHeaders", () => {
  it("does not force a content type on bodyless requests", () => {
    expect(requestHeaders().has("Content-Type")).toBe(false);
  });

  it("defaults request bodies to JSON without overriding explicit types", () => {
    expect(
      requestHeaders({
        method: "POST",
        body: JSON.stringify({ ok: true }),
      }).get("Content-Type"),
    ).toBe("application/json");
    expect(
      requestHeaders({
        method: "POST",
        headers: { "Content-Type": "text/csv" },
        body: "name\nexample",
      }).get("Content-Type"),
    ).toBe("text/csv");
  });

  it("continues when browser storage is unavailable", () => {
    const descriptor = Object.getOwnPropertyDescriptor(window, "localStorage");
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      get: () => {
        throw new DOMException("Storage is disabled", "SecurityError");
      },
    });
    try {
      expect(requestHeaders().has("X-AppStruct-Tenant")).toBe(false);
      expect(tenantApi.current()).toBeUndefined();
      expect(() => tenantApi.select("tenant-1")).not.toThrow();
      expect(() => tenantApi.clear()).not.toThrow();
    } finally {
      if (descriptor) Object.defineProperty(window, "localStorage", descriptor);
      else delete (window as { localStorage?: Storage }).localStorage;
    }
  });
});

const actor = { id: "user-1", roles: ["member"] };
const admin = { id: "admin-1", roles: ["admin"] };

function field(
  overrides: Partial<FieldDefinition> & Pick<FieldDefinition, "name" | "kind">,
): FieldDefinition {
  return {
    label: overrides.name,
    required: false,
    readOnly: false,
    primaryKey: false,
    searchable: false,
    filterable: false,
    sortable: false,
    ...overrides,
  };
}

function resource(
  overrides: Partial<ResourceDefinition> = {},
): ResourceDefinition {
  return {
    id: "app::Note",
    name: "Note",
    eventPrefix: "note",
    label: "Notes",
    slug: "notes",
    primaryKey: "id",
    softDelete: false,
    access: {
      list: { mode: "public" },
      read: { mode: "authenticated" },
      create: { mode: "role", role: "member" },
      update: { mode: "owner", field: "owner" },
      delete: { mode: "all", rules: [{ mode: "role", role: "admin" }] },
    },
    fields: [],
    api: {} as ResourceDefinition["api"],
    ...overrides,
  };
}

describe("canAccessRule", () => {
  it("evaluates public, authenticated, role, and composite rules", () => {
    expect(canAccessRule({ mode: "public" }, null)).toBe(true);
    expect(canAccessRule({ mode: "authenticated" }, null)).toBe(false);
    expect(canAccessRule({ mode: "authenticated" }, actor)).toBe(true);
    expect(canAccessRule({ mode: "role", role: "admin" }, actor)).toBe(false);
    expect(canAccessRule({ mode: "role", role: "admin" }, admin)).toBe(true);
    expect(
      canAccessRule(
        {
          mode: "any",
          rules: [{ mode: "role", role: "admin" }, { mode: "authenticated" }],
        },
        actor,
      ),
    ).toBe(true);
    expect(
      canAccessRule(
        {
          mode: "all",
          rules: [{ mode: "role", role: "admin" }, { mode: "authenticated" }],
        },
        actor,
      ),
    ).toBe(false);
  });

  it("matches owner fields on the record, including relation ids", () => {
    expect(canAccessRule({ mode: "owner", field: "owner" }, null, {})).toBe(
      false,
    );
    expect(canAccessRule({ mode: "owner", field: "owner" }, actor)).toBe(true);
    expect(
      canAccessRule({ mode: "owner", field: "owner" }, actor, {
        owner: "user-1",
      }),
    ).toBe(true);
    expect(
      canAccessRule({ mode: "owner", field: "owner" }, actor, {
        owner_id: "user-1",
      }),
    ).toBe(true);
    expect(
      canAccessRule({ mode: "owner", field: "note.owner" }, actor, {
        owner_id: "user-1",
      }),
    ).toBe(true);
    expect(
      canAccessRule({ mode: "owner", field: "owner" }, actor, {
        owner: "other",
      }),
    ).toBe(false);
  });
});

describe("canAccessResource", () => {
  it("uses the operation access rule", () => {
    const notes = resource();
    expect(canAccessResource(notes, "list", null)).toBe(true);
    expect(canAccessResource(notes, "read", null)).toBe(false);
    expect(canAccessResource(notes, "create", actor)).toBe(true);
    expect(canAccessResource(notes, "update", actor, { owner: "user-1" })).toBe(
      true,
    );
    expect(canAccessResource(notes, "delete", actor)).toBe(false);
    expect(canAccessResource(notes, "delete", admin)).toBe(true);
  });
});

describe("fieldErrors and errorMessage", () => {
  it("extracts field violations and falls back for unknown errors", () => {
    const error = new ApiError(422, "VALIDATION", "invalid", [
      { field: "title", message: "required" },
    ]);
    expect(fieldErrors(error)).toEqual({ title: "required" });
    expect(fieldErrors({})).toEqual({});
    expect(errorMessage(error)).toBe("invalid");
    expect(errorMessage("nope")).toBe("The request could not be completed");
  });
});

describe("resourceQueryKeys", () => {
  it("nests list and detail keys under the resource", () => {
    expect(resourceQueryKeys.list("app::Note", "page=1")).toEqual([
      "resource",
      "app::Note",
      "list",
      "page=1",
    ]);
    expect(resourceQueryKeys.detail("app::Note", "abc")).toEqual([
      "resource",
      "app::Note",
      "detail",
      "abc",
    ]);
    expect(resourceQueryKeys.options("app::Note")).toEqual([
      "resource",
      "app::Note",
      "options",
      "",
    ]);
  });
});

describe("appQueryKeys", () => {
  it("includes pagination and filters in admin cache keys", () => {
    expect(appQueryKeys.admin.jobs("dead", 2, 25)).toEqual([
      "admin",
      "jobs",
      { status: "dead", page: 2, pageSize: 25 },
    ]);
    expect(appQueryKeys.admin.users(3, 50)).toEqual([
      "admin",
      "users",
      { page: 3, pageSize: 50 },
    ]);
  });
});

describe("resource filters and display", () => {
  it("builds filter and range query values from search params", () => {
    const status = field({ name: "status", kind: "enum", filterable: true });
    const createdAt = field({
      name: "created_at",
      kind: "datetime",
      filterable: true,
    });
    expect(supportsRange(status)).toBe(false);
    expect(supportsRange(createdAt)).toBe(true);
    const params = new URLSearchParams(
      "filter[status]=open&filter[created_at][gte]=2026-01-01&filter[created_at][lte]=2026-12-31",
    );
    expect(buildResourceFilterQuery([status, createdAt], params)).toEqual({
      filters: { status: "open", created_at: "" },
      range_filters: {
        created_at: { gte: "2026-01-01", lte: "2026-12-31" },
      },
    });
  });

  it("formats empty, boolean, and object values for the table", () => {
    expect(formatValue(null)).toBe("-");
    expect(formatValue("")).toBe("-");
    expect(formatValue(true)).toBe("Yes");
    expect(formatValue(false)).toBe("No");
    expect(formatValue({ a: 1 })).toBe('{"a":1}');
    expect(formatValue("ready")).toBe("ready");
  });

  it("allows inline edit only for simple writable fields", () => {
    expect(supportsInlineEdit(field({ name: "title", kind: "string" }))).toBe(
      true,
    );
    expect(
      supportsInlineEdit(field({ name: "id", kind: "uuid", primaryKey: true })),
    ).toBe(false);
    expect(supportsInlineEdit(field({ name: "notes", kind: "json" }))).toBe(
      false,
    );
    expect(
      supportsInlineEdit(
        field({ name: "title", kind: "string", readOnly: true }),
      ),
    ).toBe(false);
  });
});
