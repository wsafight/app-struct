import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, rs } from "@rstest/core";
import {
  act,
  cleanup,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
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
import { ResourceFilters } from "./pages/ResourceFilters";
import { useCsvTransfer } from "./pages/resource-list/useCsvTransfer";

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
    expect(parsed.cursorMode).toBe(false);
    expect(
      "page_size" in parsed.query ? parsed.query.page_size : undefined,
    ).toBe(25);
    expect(parsed.query.range_filters.amount.gte).toBe("0.1");
    invoice.displayField = "number";
    expect(recordLabel(invoice, { id: "one", number: "INV-001" })).toBe(
      "INV-001",
    );
    expect(recordLabel(invoice, { id: "one" })).toBe("one");
  });

  it("uses signed cursor pagination only when explicitly requested, including sorting", () => {
    const parsed = parseResourceQuery(
      resource(),
      [amount],
      new URLSearchParams("pagination=cursor&sort=-amount"),
    );
    expect(parsed.cursorMode).toBe(true);
    expect(parsed.query.sort).toBe("-amount");
    expect("limit" in parsed.query ? parsed.query.limit : undefined).toBe(25);
  });

  it("preserves page numbers and nested advanced filters in sorted views", () => {
    const expression = {
      any: [
        { field: "amount", op: "gte", values: ["0"] },
        { all: [{ field: "amount", op: "is_null" }] },
      ],
    };
    const parsed = parseResourceQuery(
      resource(),
      [amount],
      new URLSearchParams({
        page: "3",
        sort: "-amount",
        where: JSON.stringify(expression),
      }),
    );
    expect(parsed.cursorMode).toBe(false);
    expect(parsed.page).toBe(3);
    expect(parsed.query.where).toEqual(expression);
  });

  it("applies advanced OR conditions with zero and null values", () => {
    const updateParam = rs.fn();
    render(
      <ResourceFilters
        fields={[amount]}
        resources={[]}
        searchParams={new URLSearchParams()}
        updateParam={updateParam}
      />,
    );
    fireEvent.change(screen.getByLabelText("Filter match mode"), {
      target: { value: "any" },
    });
    fireEvent.click(screen.getByText("Add condition"));
    fireEvent.change(screen.getByLabelText("Condition 1 operator"), {
      target: { value: "gte" },
    });
    fireEvent.change(screen.getByLabelText("Condition 1 value"), {
      target: { value: "0" },
    });
    fireEvent.click(screen.getByText("Add condition"));
    fireEvent.change(screen.getByLabelText("Condition 2 operator"), {
      target: { value: "is_null" },
    });
    fireEvent.click(screen.getByText("Apply filters"));
    expect(updateParam).toHaveBeenCalledWith(
      "where",
      JSON.stringify({
        any: [
          { field: "amount", op: "gte", values: ["0"] },
          { field: "amount", op: "is_null", values: [] },
        ],
      }),
      false,
    );
  });

  it("shows facet counts for the current advanced filter", async () => {
    const invoice = resource();
    invoice.api.facets = rs.fn().mockResolvedValue({
      data: {
        paid: { buckets: [{ value: true, count: 3 }], other_count: 2 },
      },
    });
    const paid: FieldDefinition = {
      ...amount,
      name: "paid",
      apiName: "paid",
      label: "Paid",
      kind: "boolean",
    };
    const expression = { field: "amount", op: "gte", values: ["10"] };
    render(
      <ResourceFilters
        resource={invoice}
        fields={[amount, paid]}
        resources={[]}
        searchParams={
          new URLSearchParams({ where: JSON.stringify(expression) })
        }
        updateParam={rs.fn()}
      />,
      { wrapper: wrapper() },
    );
    await waitFor(() =>
      expect(
        screen.getByLabelText("Counts for current filters").textContent,
      ).toContain("true (3) · Other (2)"),
    );
    expect(invoice.api.facets).toHaveBeenCalledWith(
      expect.objectContaining({ facet: ["paid"], where: expression }),
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
  });

  it("passes filters and projected columns to the streaming CSV download", async () => {
    const invoice = resource();
    invoice.api.downloadCsv = rs.fn().mockResolvedValue(undefined);
    const expression = { field: "amount", op: "gte" as const, values: ["0"] };
    const { result } = renderHook(
      () =>
        useCsvTransfer({
          resource: invoice,
          runChange: rs.fn(),
          onError: rs.fn(),
          query: {
            page: 3,
            fields: ["amount"],
            where: expression,
            sort: "-amount",
          },
        }),
      { wrapper: wrapper() },
    );
    await act(async () => {
      result.current.exportCsv();
    });
    await waitFor(() => expect(invoice.api.downloadCsv).toHaveBeenCalledOnce());
    expect(invoice.api.downloadCsv).toHaveBeenCalledWith(
      expect.objectContaining({
        fields: ["amount"],
        where: expression,
        sort: "-amount",
      }),
    );
    expect(invoice.api.downloadCsv).toHaveBeenCalledWith(
      expect.not.objectContaining({ page: 3 }),
    );
    expect(invoice.api.exportCsv).not.toHaveBeenCalled();
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
