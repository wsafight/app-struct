import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, rs } from "@rstest/core";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import * as resourceActual from "./resource" with { rstest: "importActual" };
import {
  useResourceFormController,
  useResourceListController,
} from "./controller";
import { useRelationOptions } from "./relation-options";
import { recordLabel } from "./relations";
import type { FieldDefinition, ResourceDefinition } from "./resource";
import { parseResourceQuery } from "./url-controller";

rs.mock("./resource", () => ({
  ...resourceActual,
  useResourceActor: () => null,
  useCanAccess: () => true,
}));
afterEach(cleanup);

const amount: FieldDefinition = {
  name: "amount",
  apiName: "amount",
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
      get: rs.fn(),
      list: rs.fn(),
      create: rs.fn(),
      update: rs.fn(),
      remove: rs.fn(),
      aggregate: rs.fn(),
      listCursor: rs.fn(),
      bulkUpdate: rs.fn(),
      bulkDelete: rs.fn(),
      exportCsv: rs.fn(),
      importCsv: rs.fn(),
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
    rs.mocked(invoice.api.update)
      .mockRejectedValueOnce({ code: "CONCURRENT_MODIFICATION" })
      .mockResolvedValueOnce({
        id: "one",
        amount: "9007199254740993.15",
        revision: 3,
      });
    const refetchRecord = rs
      .fn()
      .mockResolvedValue({ id: "one", amount: "2.00", revision: 2 });
    const onSaved = rs.fn();
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
    rs.mocked(invoice.api.list).mockImplementation(async (query) => ({
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

  it("uses cursor metadata without requesting a total count", async () => {
    const invoice = resource();
    rs.mocked(invoice.api.listCursor).mockResolvedValue({
      data: [{ id: "one" }],
      meta: {
        limit: 25,
        next_cursor: "next",
        previous_cursor: null,
        has_more: true,
      },
    });
    const { result } = renderHook(
      () =>
        useResourceListController(invoice, {
          cacheKey: "cursor",
          cursorMode: true,
          query: { limit: 25 },
        }),
      { wrapper: wrapper() },
    );
    await waitFor(() => expect(result.current.records[0]?.id).toBe("one"));
    expect(result.current.total).toBe(0);
    expect(result.current.nextCursor).toBe("next");
    expect(result.current.previousCursor).toBeNull();
    expect(invoice.api.list).not.toHaveBeenCalled();
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
    expect(parsed.cursorMode).toBe(true);
    expect("limit" in parsed.query ? parsed.query.limit : undefined).toBe(25);
    expect(parsed.query.range_filters.amount.gte).toBe("0.1");
    invoice.displayField = "number";
    expect(recordLabel(invoice, { id: "one", number: "INV-001" })).toBe(
      "INV-001",
    );
    expect(recordLabel(invoice, { id: "one" })).toBe("one");
  });

  it("keeps relation controls usable when option loading is unauthorized", () => {
    const invoice = resource();
    invoice.access.list = { mode: "authenticated" };
    const { result } = renderHook(() => useRelationOptions(invoice, "one"), {
      wrapper: wrapper(),
    });
    expect(result.current.canLoad).toBe(false);
    expect(result.current.isPending).toBe(true);
    expect(invoice.api.list).not.toHaveBeenCalled();
    expect(invoice.api.get).not.toHaveBeenCalled();
  });
});
