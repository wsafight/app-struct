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
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import {
  canAccessResource,
  listProjectionFields,
  type FieldDefinition,
  type ResourceDefinition,
} from "./resource";
import { formatFieldValue } from "./pages/resource-list/ResourceTable";
import { CsvImportDialog } from "./pages/resource-list/CsvImportDialog";
import { parseCsv, writeCsv } from "./pages/resource-list/csv-import";
import { AdvancedFilters } from "./pages/resource-list/AdvancedFilters";
import { useBulkActions } from "./pages/resource-list/BulkActions";
import { reportDrilldown } from "./pages/resource-list/report-query";
import { aggregateChartRows } from "./pages/resource-list/ResourceInsights";
import { validateResourceSearch } from "./navigation";

afterEach(cleanup);
function field(
  name: string,
  kind: FieldDefinition["kind"] = "string",
  extra: Partial<FieldDefinition> = {},
): FieldDefinition {
  return {
    name,
    apiName: name,
    label: name === "title" ? "Title" : name,
    kind,
    required: false,
    primaryKey: name === "id",
    readOnly: name === "id",
    searchable: false,
    filterable: true,
    sortable: true,
    ...extra,
  };
}
function resource(fields: FieldDefinition[]): ResourceDefinition {
  return {
    id: "test::Record",
    name: "Record",
    label: "Records",
    slug: "records",
    eventPrefix: "record",
    primaryKey: "id",
    softDelete: false,
    fields,
    access: {
      list: { mode: "public" },
      read: { mode: "public" },
      create: { mode: "public" },
      update: { mode: "public" },
      delete: { mode: "public" },
    },
    api: {
      list: rs.fn(),
      listCursor: rs.fn(),
      aggregate: rs.fn(),
      get: rs.fn(),
      create: rs.fn(),
      update: rs.fn(),
      remove: rs.fn(),
      bulkUpdate: rs.fn(),
      bulkDelete: rs.fn(),
      exportCsv: rs.fn(),
      importCsv: rs.fn(),
    },
  };
}
const runChange = async (operation: () => Promise<void>) => {
  await operation();
  return true;
};
function wrapper({ children }: { children: ReactNode }) {
  return (
    <QueryClientProvider
      client={
        new QueryClient({
          defaultOptions: {
            queries: { retry: false },
            mutations: { retry: false },
          },
        })
      }
    >
      {children}
    </QueryClientProvider>
  );
}

describe("list projections", () => {
  it("keeps hidden money and nested owner dependencies without exposing restricted fields", () => {
    const amount = field("amount", "decimal", {
      semantic: { kind: "money", currencyField: "currency", fractionDigits: 2 },
    });
    const model = resource([
      field("id", "uuid"),
      amount,
      field("currency", "enum"),
      field("owner_id", "uuid"),
      field("secret", "text", { readAccess: { mode: "role", role: "admin" } }),
    ]);
    model.access.delete = {
      mode: "all",
      rules: [
        { mode: "role", role: "operator" },
        { mode: "owner", field: "test::Record.owner" },
      ],
    };
    const actor = { id: "one", roles: ["operator"] };
    const names = listProjectionFields(model, [amount], actor);
    expect(names).toEqual(["id", "amount", "currency", "owner_id"]);
    const full = {
      id: "record",
      amount: "9007199254740993.15",
      currency: "USD",
      owner_id: "one",
      secret: "hidden",
    };
    const projected = Object.fromEntries(
      Object.entries(full).filter(([name]) => names.includes(name)),
    );
    expect(formatFieldValue(projected, amount)).toContain("USD");
    expect(canAccessResource(model, "delete", actor, projected)).toBe(true);
  });
});

describe("CSV workflow", () => {
  it("preserves quoted multiline cells, BOMs, blank lines and empty column rows", () => {
    const rows = [
      ["title", "note"],
      ["One, two", 'say "hi"\r\nnext'],
      ["", ""],
    ];
    expect(parseCsv("\uFEFF\r\n" + writeCsv(rows))).toEqual(rows);
    expect(() => parseCsv('title\n"closed"extra')).toThrow();
    expect(() => parseCsv("title,title\n1,2")).toThrow();
  });
  it("requires mapping and review, then retries only corrected failed rows", async () => {
    const model = resource([
      field("title", "string", { required: true }),
      field("status", "enum", { required: true, values: ["todo", "done"] }),
    ]);
    rs.mocked(model.api.importCsv)
      .mockResolvedValueOnce({
        succeeded: ["0"],
        failed: [
          { id: "1", code: "validation_failed", message: "Duplicate title" },
        ],
      })
      .mockResolvedValueOnce({ succeeded: ["0"], failed: [] });
    const user = userEvent.setup();
    render(
      <CsvImportDialog
        resource={model}
        file={{
          name: "records.csv",
          text: 'Name,Status\n"First, quoted",todo\nSecond,done\n',
        }}
        runChange={runChange}
        onError={rs.fn()}
        onClose={rs.fn()}
      />,
      { wrapper },
    );
    expect(model.api.importCsv).not.toHaveBeenCalled();
    expect(
      screen
        .getByRole("button", { name: "Import 2 rows" })
        .hasAttribute("disabled"),
    ).toBe(true);
    await user.selectOptions(screen.getByLabelText("Map Name"), "title");
    await user.click(screen.getByRole("button", { name: "Import 2 rows" }));
    await waitFor(() =>
      expect(screen.getByText("Duplicate title")).toBeTruthy(),
    );
    expect(parseCsv(rs.mocked(model.api.importCsv).mock.calls[0][0])).toEqual([
      ["title", "status"],
      ["First, quoted", "todo"],
      ["Second", "done"],
    ]);
    fireEvent.change(screen.getByLabelText("Row 3 Title"), {
      target: { value: "Corrected" },
    });
    await user.click(
      screen.getByRole("button", { name: "Retry remaining rows" }),
    );
    await waitFor(() =>
      expect(screen.getByText("All rows were imported.")).toBeTruthy(),
    );
    expect(parseCsv(rs.mocked(model.api.importCsv).mock.calls[1][0])).toEqual([
      ["title", "status"],
      ["Corrected", "done"],
    ]);
  });
});

describe("nested filters", () => {
  it("edits one nested condition and preserves its sibling and outer group", async () => {
    const fields = [
      field("status", "enum", { values: ["todo", "done"] }),
      field("score", "integer"),
    ];
    const expression = {
      all: [
        { field: "status", op: "eq", values: ["todo"] },
        {
          any: [
            { field: "score", op: "gte", values: ["5"] },
            { field: "score", op: "is_null" },
          ],
        },
      ],
    };
    const apply = rs.fn();
    render(
      <AdvancedFilters
        fields={fields}
        raw={JSON.stringify(expression)}
        onApply={apply}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByText("Advanced filters · active"));
    fireEvent.change(screen.getByLabelText("Condition 2.1 value"), {
      target: { value: "7" },
    });
    await user.click(screen.getByRole("button", { name: "Apply filters" }));
    expect(JSON.parse(apply.mock.calls[0][0])).toEqual({
      all: [
        expression.all[0],
        {
          any: [
            { field: "score", op: "gte", values: ["7"] },
            { field: "score", op: "is_null" },
          ],
        },
      ],
    });
  });
  it("locates invalid input and blocks submission", async () => {
    const apply = rs.fn();
    render(
      <AdvancedFilters
        fields={[field("score", "bigint")]}
        raw={null}
        onApply={apply}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByText("Advanced filters"));
    await user.click(screen.getByRole("button", { name: "Add condition" }));
    fireEvent.change(screen.getByLabelText("Condition 1 value"), {
      target: { value: "9223372036854775808" },
    });
    await user.click(screen.getByRole("button", { name: "Apply filters" }));
    expect(screen.getByText(/Condition 1:.*bigint range/)).toBeTruthy();
    expect(apply).not.toHaveBeenCalled();
  });
});

describe("bulk review and retry", () => {
  it("preserves bigint precision and reviews a retry with refreshed revisions", async () => {
    const model = resource([field("amount", "bigint")]);
    model.api.lookup = rs.fn().mockResolvedValue([{ id: "b", revision: 9 }]);
    rs.mocked(model.api.bulkUpdate)
      .mockResolvedValueOnce({
        succeeded: ["a"],
        failed: [
          { id: "b", code: "concurrent_modification", message: "Changed" },
        ],
      })
      .mockResolvedValueOnce({ succeeded: ["b"], failed: [] });
    const { result, rerender } = renderHook(
      ({ selected }) =>
        useBulkActions({
          resource: model,
          records: [
            { id: "a", revision: 1 },
            { id: "b", revision: 2 },
          ],
          rowSelection: selected,
          trashMode: false,
          actor: null,
          runChange,
          onError: rs.fn(),
        }),
      {
        initialProps: {
          selected: { a: true, b: true } as Record<string, true>,
        },
      },
    );
    act(() => result.current.setBulkValue("9007199254740993"));
    act(() => result.current.bulkUpdate());
    expect(model.api.bulkUpdate).not.toHaveBeenCalled();
    await act(() => result.current.execute());
    expect(model.api.bulkUpdate).toHaveBeenCalledWith({
      ids: ["a", "b"],
      patch: { amount: "9007199254740993" },
      expected_revisions: { a: 1, b: 2 },
    });
    rerender({ selected: {} });
    expect(result.current.outcome?.result.failed[0].id).toBe("b");
    await act(() => result.current.reviewRetry());
    expect(model.api.bulkUpdate).toHaveBeenCalledOnce();
    await act(() => result.current.execute());
    expect(model.api.bulkUpdate).toHaveBeenLastCalledWith({
      ids: ["b"],
      patch: { amount: "9007199254740993" },
      expected_revisions: { b: 9 },
    });
  });
});

describe("reports", () => {
  it("links a UTC month to matching page results while preserving existing conditions", () => {
    const existing = { field: "status", op: "ne", values: ["done"] };
    const params = new URLSearchParams({
      where: JSON.stringify(existing),
      q: "invoice",
      page: "4",
      summary: "1",
      report_group: "created_at",
      report_bucket: "month",
    });
    const link = new URLSearchParams(
      reportDrilldown(params, "created_at", "2026-12-01", "month", "datetime"),
    );
    expect(link.has("page")).toBe(false);
    expect(link.has("summary")).toBe(false);
    expect(link.get("q")).toBe("invoice");
    expect(JSON.parse(link.get("where")!)).toEqual({
      all: [
        existing,
        {
          all: [
            {
              field: "created_at",
              op: "gte",
              values: ["2026-12-01T00:00:00.000Z"],
            },
            {
              field: "created_at",
              op: "lt",
              values: ["2027-01-01T00:00:00.000Z"],
            },
          ],
        },
      ],
    });
    expect(
      validateResourceSearch(Object.fromEntries(params)).report_bucket,
    ).toBe("month");
  });
  it("keeps exact tables when a chart would round a large aggregate", () => {
    expect(
      aggregateChartRows(
        [{ group_status: "todo", sum_amount: "9007199254740993" }],
        "status",
        "sum_amount",
      ),
    ).toBeNull();
    expect(
      aggregateChartRows(
        [{ group_status: "todo", sum_amount: null }],
        "status",
        "sum_amount",
      ),
    ).toBeNull();
  });
});
