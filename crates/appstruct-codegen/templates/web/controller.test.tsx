import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  useResourceFormController,
  useResourceListController,
} from "./controller";
import { recordLabel } from "./relations";
import type { FieldDefinition, ResourceDefinition } from "./resource";
import { parseResourceQuery } from "./url-controller";

vi.mock("./resource", async (original) => ({
  ...(await original<typeof import("./resource")>()),
  useResourceActor: () => null,
  useCanAccess: () => true,
}));
afterEach(cleanup);

const amount: FieldDefinition = {
  name: "amount",
  label: "Amount",
  kind: "decimal",
  required: true,
  primaryKey: false,
  readOnly: false,
  searchable: false,
  filterable: true,
  sortable: true,
};
function resource(): ResourceDefinition {
  return {
    id: "app::Invoice",
    name: "Invoice",
    label: "Invoices",
    slug: "invoices",
    eventPrefix: "invoice",
    primaryKey: "id",
    softDelete: false,
    fields: [amount],
    access: {
      list: { mode: "public" },
      read: { mode: "public" },
      create: { mode: "public" },
      update: { mode: "public" },
      delete: { mode: "public" },
    },
    api: {
      get: vi.fn(),
      list: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      remove: vi.fn(),
      aggregate: vi.fn(),
      listCursor: vi.fn(),
      bulkUpdate: vi.fn(),
      bulkDelete: vi.fn(),
      exportCsv: vi.fn(),
      importCsv: vi.fn(),
    },
  };
}
function wrapper() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
  };
}

describe("headless controllers", () => {
  it("preserves the draft after a conflict and explicitly reloads the latest baseline", async () => {
    const invoice = resource();
    vi.mocked(invoice.api.update)
      .mockRejectedValueOnce({ code: "CONCURRENT_MODIFICATION" })
      .mockResolvedValueOnce({
        id: "one",
        amount: "9007199254740993.15",
        revision: 3,
      });
    const refetchRecord = vi
      .fn()
      .mockResolvedValue({ id: "one", amount: "2.00", revision: 2 });
    const onSaved = vi.fn();
    const { result } = renderHook(
      () =>
        useResourceFormController(invoice, {
          id: "one",
          initialRecord: { id: "one", amount: "1.00", revision: 1 },
          refetchRecord,
          onSaved,
        }),
      { wrapper: wrapper() },
    );
    await act(async () => {
      result.current.form.setFieldValue("amount", "9007199254740993.15");
      await result.current.form.handleSubmit();
    });
    await waitFor(() => expect(result.current.conflict).toBe(true));
    expect(result.current.form.state.values.amount).toBe("9007199254740993.15");
    expect(result.current.form.state.isDirty).toBe(true);
    expect(onSaved).not.toHaveBeenCalled();
    await act(async () => {
      await result.current.reloadRecord();
    });
    expect(result.current.conflict).toBe(false);
    expect(result.current.form.state.values.amount).toBe("2.00");
    expect(result.current.form.state.isDirty).toBe(false);
    await act(async () => {
      result.current.form.setFieldValue("amount", "9007199254740993.15");
      await result.current.form.handleSubmit();
    });
    expect(invoice.api.update).toHaveBeenLastCalledWith("one", {
      amount: "9007199254740993.15",
    });
    expect(onSaved).toHaveBeenCalledOnce();
    expect(result.current.form.state.isDirty).toBe(false);
  });

  it("refetches when query input changes under the same caller cache key", async () => {
    const invoice = resource();
    vi.mocked(invoice.api.list).mockImplementation(async (query) => ({
      data: [{ id: query?.q }],
      meta: { page: 1, page_size: 25, total: 1 },
    }));
    const { result, rerender } = renderHook(
      ({ q }) =>
        useResourceListController(invoice, {
          cacheKey: "custom",
          query: { q },
        }),
      { initialProps: { q: "first" }, wrapper: wrapper() },
    );
    await waitFor(() => expect(result.current.records[0]?.id).toBe("first"));
    rerender({ q: "second" });
    await waitFor(() => expect(result.current.records[0]?.id).toBe("second"));
  });

  it("uses shared URL defaults and falls back for redacted labels", () => {
    const invoice = resource();
    const parsed = parseResourceQuery(
      invoice,
      [amount],
      new URLSearchParams("page=0&page_size=900&filter[amount][gte]=0.1"),
    );
    expect(parsed.page).toBe(1);
    expect(parsed.pageSize).toBe(25);
    expect(parsed.query.range_filters.amount.gte).toBe("0.1");
    invoice.displayField = "number";
    expect(recordLabel(invoice, { id: "one", number: "INV-001" })).toBe(
      "INV-001",
    );
    expect(recordLabel(invoice, { id: "one" })).toBe("one");
  });
});
