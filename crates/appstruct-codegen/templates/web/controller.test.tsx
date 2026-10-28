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
import userEvent from "@testing-library/user-event";
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
      importResult: rs.fn(),
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

function FormHarness({ invoice }: { invoice: ResourceDefinition }) {
  const { form, fields } = useResourceFormController(invoice);
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void form.handleSubmit();
      }}
    >
      {fields.map((definition) => (
        <form.Field key={definition.name} name={definition.name}>
          {(field) => (
            <label>
              {definition.label}
              <input
                aria-label={definition.label}
                value={String(field.state.value)}
                onChange={(event) => field.handleChange(event.target.value)}
              />
              {field.state.meta.errors.length > 0 && (
                <span role="alert">{field.state.meta.errors[0]}</span>
              )}
            </label>
          )}
        </form.Field>
      ))}
      <button type="submit">Save</button>
    </form>
  );
}

describe("headless controllers", () => {
  it("sends required false values on create even when the checkbox was untouched", async () => {
    const invoice = resource();
    invoice.fields.push({
      ...amount,
      name: "paid",
      apiName: "paid",
      label: "Paid",
      kind: "boolean",
    });
    rs.mocked(invoice.api.create).mockResolvedValue({
      id: "one",
      amount: "1",
      paid: false,
      revision: 1,
    });
    const { result } = renderHook(() => useResourceFormController(invoice), {
      wrapper: wrapper(),
    });
    await act(async () => {
      result.current.form.setFieldValue("amount", "1");
      await result.current.form.handleSubmit();
    });
    expect(invoice.api.create).toHaveBeenCalledWith({
      amount: "1",
      paid: false,
    });
  });

  it("shows scalar field errors, blocks invalid submissions and saves corrected values", async () => {
    const user = userEvent.setup();
    const invoice = resource();
    const reference = "01990000-0000-7000-8000-000000000001";
    invoice.fields.push({
      ...amount,
      name: "reference",
      apiName: "reference",
      label: "Reference",
      kind: "uuid",
    });
    rs.mocked(invoice.api.create).mockResolvedValue({
      id: "one",
      amount: "9007199254740993.15",
      reference,
    });
    render(<FormHarness invoice={invoice} />, { wrapper: wrapper() });
    fireEvent.change(screen.getByLabelText("Reference"), {
      target: { value: "invalid" },
    });
    await user.click(screen.getByText("Save"));
    await screen.findByText("Amount is required");
    expect(screen.getByText("Reference must be a valid UUID")).toBeTruthy();
    expect(invoice.api.create).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("Amount"), {
      target: { value: "NaN" },
    });
    await user.click(screen.getByText("Save"));
    await screen.findByText("Amount must be a decimal number");
    expect(invoice.api.create).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("Amount"), {
      target: { value: "9007199254740993.15" },
    });
    fireEvent.change(screen.getByLabelText("Reference"), {
      target: { value: reference },
    });
    await user.click(screen.getByText("Save"));
    await waitFor(() =>
      expect(invoice.api.create).toHaveBeenCalledExactlyOnceWith({
        amount: "9007199254740993.15",
        reference,
      }),
    );
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  it("validates both sides of optional money pairs and accepts zero or an empty pair", async () => {
    const user = userEvent.setup();
    const invoice = resource();
    invoice.fields = [
      {
        ...amount,
        required: false,
        semantic: {
          kind: "money",
          currencyField: "currency",
          fractionDigits: 2,
        },
      },
      {
        ...amount,
        name: "currency",
        apiName: "currency",
        label: "Currency",
        kind: "enum",
        values: ["USD", "EUR"],
        required: false,
      },
    ];
    rs.mocked(invoice.api.create).mockResolvedValue({
      id: "one",
      amount: "0",
      currency: "USD",
    });
    render(<FormHarness invoice={invoice} />, { wrapper: wrapper() });
    fireEvent.change(screen.getByLabelText("Amount"), {
      target: { value: "0" },
    });
    await user.click(screen.getByText("Save"));
    await screen.findByText("Amount and currency must be provided together");
    expect(invoice.api.create).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("Amount"), {
      target: { value: "" },
    });
    fireEvent.change(screen.getByLabelText("Currency"), {
      target: { value: "USD" },
    });
    await user.click(screen.getByText("Save"));
    await waitFor(() =>
      expect(
        screen.getByLabelText("Amount").parentElement?.textContent,
      ).toContain("Amount and currency must be provided together"),
    );
    expect(invoice.api.create).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("Amount"), {
      target: { value: "0" },
    });
    await user.click(screen.getByText("Save"));
    await waitFor(() =>
      expect(invoice.api.create).toHaveBeenCalledExactlyOnceWith({
        amount: "0",
        currency: "USD",
      }),
    );
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());

    fireEvent.change(screen.getByLabelText("Amount"), {
      target: { value: "" },
    });
    fireEvent.change(screen.getByLabelText("Currency"), {
      target: { value: "" },
    });
    await user.click(screen.getByText("Save"));
    await waitFor(() =>
      expect(invoice.api.create).toHaveBeenLastCalledWith({
        amount: null,
        currency: null,
      }),
    );
  });

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
    expect(invoice.api.update).toHaveBeenLastCalledWith(
      "one",
      {
        amount: "9007199254740993.15",
      },
      { revision: 2 },
    );
    expect(onSaved).toHaveBeenCalledOnce();
    expect(result.current.form.state.isDirty).toBe(false);
  });

  it("merges nonconflicting changes, requires explicit conflicts and pins the compared revision", async () => {
    const invoice = resource();
    invoice.fields.push({
      ...invoice.fields.find((field) => field.name === "amount")!,
      name: "note",
      apiName: "note",
      label: "Note",
      kind: "string",
    });
    rs.mocked(invoice.api.update)
      .mockRejectedValueOnce({ code: "CONCURRENT_MODIFICATION" })
      .mockResolvedValueOnce({
        id: "one",
        amount: "3.00",
        note: "Server note",
        revision: 3,
      });
    const refetchRecord = rs.fn().mockResolvedValue({
      id: "one",
      amount: "2.00",
      note: "Server note",
      revision: 2,
    });
    const { result } = renderHook(
      () =>
        useResourceFormController(invoice, {
          id: "one",
          initialRecord: {
            id: "one",
            amount: "1.00",
            note: "Original note",
            revision: 1,
          },
          refetchRecord,
        }),
      { wrapper: wrapper() },
    );
    await act(async () => {
      result.current.form.setFieldValue("amount", "3.00");
      await result.current.form.handleSubmit();
    });
    await act(async () => {
      await result.current.compareConflict();
    });
    expect(result.current.form.state.values.amount).toBe("3.00");
    expect(
      result.current.comparison?.fields.find((field) => field.name === "amount")
        ?.choice,
    ).toBeUndefined();
    expect(
      result.current.comparison?.fields.find((field) => field.name === "note")
        ?.choice,
    ).toBe("latest");
    act(() => result.current.applyConflict());
    expect(result.current.conflict).toBe(true);
    act(() => result.current.chooseConflict("amount", "mine"));
    act(() => result.current.applyConflict());
    expect(result.current.conflict).toBe(false);
    expect(result.current.form.state.values).toMatchObject({
      amount: "3.00",
      note: "Server note",
    });
    expect(result.current.form.state.isDirty).toBe(true);
    await act(async () => {
      await result.current.form.handleSubmit();
    });
    expect(invoice.api.update).toHaveBeenLastCalledWith(
      "one",
      { amount: "3", note: "Server note" },
      { revision: 2 },
    );
  });

  it("compares hidden datetime precision and preserves an explicit original-time choice", async () => {
    const invoice = resource();
    invoice.fields.push({
      ...amount,
      name: "starts",
      apiName: "starts",
      label: "Starts",
      kind: "datetime",
    });
    const original = "2026-10-01T00:00:00.000001Z";
    const latest = "2026-10-01T00:00:00.000002Z";
    rs.mocked(invoice.api.update)
      .mockRejectedValueOnce({ code: "CONCURRENT_MODIFICATION" })
      .mockResolvedValue({
        id: "one",
        amount: "2",
        starts: original,
        revision: 3,
      });
    const { result } = renderHook(
      () =>
        useResourceFormController(invoice, {
          id: "one",
          initialRecord: {
            id: "one",
            amount: "1",
            starts: original,
            revision: 1,
          },
          refetchRecord: async () => ({
            id: "one",
            amount: "1",
            starts: latest,
            revision: 2,
          }),
        }),
      { wrapper: wrapper() },
    );
    const originalInput = result.current.form.state.values.starts;
    await act(async () => {
      result.current.form.setFieldValue("amount", "2");
      await result.current.form.handleSubmit();
      await result.current.compareConflict();
    });
    expect(
      result.current.comparison?.fields.find(
        (field) => field.name === "starts",
      ),
    ).toMatchObject({ original, mine: original, latest, choice: "latest" });
    act(() => result.current.chooseConflict("starts", "mine"));
    act(() => result.current.applyConflict());
    expect(result.current.form.state.values.starts).toBe(originalInput);
    await act(async () => {
      await result.current.form.handleSubmit();
    });
    expect(invoice.api.update).toHaveBeenLastCalledWith(
      "one",
      { amount: "2", starts: original },
      { revision: 2 },
    );
    // A draft with a visibly different timestamp must also resolve a server microsecond change.
    // Use a valid local timestamp regardless of the test runner's time zone.
    act(() => result.current.form.setFieldValue("starts", "2026-10-03T12:00"));
    await act(async () => {
      await result.current.compareConflict();
    });
    expect(
      result.current.comparison?.fields.find((field) => field.name === "starts")
        ?.choice,
    ).toBeUndefined();
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
      ).toContain("Yes (3) · Other (2)"),
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
