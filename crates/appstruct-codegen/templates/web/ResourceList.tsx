import type { RowSelectionState } from "@tanstack/react-table";
import {
  ChevronLeft,
  ChevronRight,
  Download,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Trash2,
  Upload,
} from "lucide-react";
import { type FormEvent, useEffect, useMemo, useState } from "react";
import { ConfirmDialog } from "../components/Dialog";
import { useResourceListController } from "../controller";
import { Link, useSearchParams } from "../navigation";
import type {
  FieldDefinition,
  ResourceDefinition,
  ResourceRecord,
} from "../resource";
import {
  canAccessRule,
  errorMessage,
  useCanAccess,
  useResourceActor,
} from "../resource";
import { useRealtimeResource } from "../realtime/useRealtimeResource";
import { buildResourceFilterQuery, ResourceFilters } from "./ResourceFilters";
import { BulkToolbar, useBulkActions } from "./resource-list/BulkActions";
import { ResourceInsights } from "./resource-list/ResourceInsights";
import { ResourceTable } from "./resource-list/ResourceTable";
import { SavedViews } from "./resource-list/SavedViews";
import { useCsvTransfer } from "./resource-list/useCsvTransfer";
import {
  defaultVisibleFieldNames,
  resolveVisibleFields,
  ViewOptions,
} from "./resource-list/ViewOptions";

export { formatValue } from "./resource-list/ResourceTable";

export function ResourceList({
  resource,
  resources,
}: {
  resource: ResourceDefinition;
  resources: ResourceDefinition[];
}) {
  const actor = useResourceActor();
  const canCreate = useCanAccess(resource, "create");
  const [searchParams, setSearchParams] = useSearchParams();
  const [search, setSearch] = useState(searchParams.get("q") ?? "");
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [actionError, setActionError] = useState("");
  const [confirmation, setConfirmation] = useState<{
    title: string;
    description: string;
    action(): Promise<void>;
  } | null>(null);
  const queryString = searchParams.toString();
  const trashMode = resource.softDelete && searchParams.get("trash") === "1";
  const page = boundedInteger(searchParams.get("page"), 1, 10_000, 1);
  const pageSize = boundedInteger(searchParams.get("page_size"), 1, 100, 25);
  const sort = searchParams.get("sort") ?? "";
  const listFields = useMemo(
    () =>
      resource.fields.filter(
        (field) =>
          field.kind !== "json" &&
          canAccessRule(field.readAccess ?? { mode: "public" }, actor),
      ),
    [actor, resource],
  );
  const visibleFields = useMemo(
    () => resolveVisibleFields(listFields, searchParams.get("columns")),
    [listFields, searchParams],
  );
  const filterFields = useMemo(
    () =>
      resource.fields.filter(
        (field) =>
          field.filterable &&
          canAccessRule(field.readAccess ?? { mode: "public" }, actor),
      ),
    [actor, resource],
  );

  const controller = useResourceListController(resource, {
    cacheKey: queryString,
    trashMode,
    query: {
      page,
      page_size: pageSize,
      sort: sort || undefined,
      q: searchParams.get("q") ?? undefined,
      ...buildResourceFilterQuery(filterFields, searchParams),
    },
    onChangeSuccess: () => setRowSelection({}),
  });
  const { records, total } = controller;

  useEffect(() => {
    setSearch(new URLSearchParams(queryString).get("q") ?? "");
    setActionError("");
    setRowSelection({});
  }, [queryString]);
  useEffect(() => {
    setRowSelection({});
  }, [controller.dataUpdatedAt]);
  useRealtimeResource({
    enabled: controller.canList,
    resourceId: resource.id,
    resourceSlug: resource.slug,
    eventPrefix: resource.eventPrefix,
  });

  function updateParam(name: string, value?: string, replace = false) {
    setSearchParams(
      (current) => {
        const next = new URLSearchParams(current);
        if (value) next.set(name, value);
        else next.delete(name);
        if (name !== "page") next.delete("page");
        return next;
      },
      { replace },
    );
  }

  function submitSearch(event: FormEvent) {
    event.preventDefault();
    updateParam("q", search.trim() || undefined);
  }

  function changeSort(field: string) {
    const next =
      sort === field ? `-${field}` : sort === `-${field}` ? undefined : field;
    updateParam("sort", next);
  }

  function changeColumns(names: string[]) {
    const defaults = defaultVisibleFieldNames(listFields);
    updateParam(
      "columns",
      names.join(",") === defaults.join(",") ? undefined : names.join(","),
    );
  }

  async function runChange(operation: () => Promise<void>): Promise<boolean> {
    setActionError("");
    return controller.runChange(operation);
  }

  async function remove(id: string) {
    const action = resource.softDelete
      ? "Move this record to trash"
      : "Delete this record";
    setConfirmation({
      title: action,
      description: "This action cannot be undone from the current view.",
      action: async () => {
        await runChange(() => resource.api.remove(id));
      },
    });
  }

  function revisionMap(ids: string[]): Record<string, number> {
    return Object.fromEntries(
      ids.map((id) => [
        id,
        Number(
          records.find((record) => String(record[resource.primaryKey]) === id)
            ?.revision ?? 0,
        ),
      ]),
    );
  }

  async function restoreOne(id: string) {
    if (!resource.api.restore) return;
    await runChange(async () => {
      const result = await resource.api.restore!({
        ids: [id],
        expected_revisions: revisionMap([id]),
      });
      if (result.failed.length) setActionError(result.failed[0].message);
    });
  }

  async function updateField(
    record: ResourceRecord,
    field: FieldDefinition,
    value: unknown,
  ): Promise<boolean> {
    const id = String(record[resource.primaryKey]);
    return runChange(async () => {
      const result = await resource.api.bulkUpdate({
        ids: [id],
        patch: { [field.name]: value },
        expected_revisions: { [id]: Number(record.revision ?? 0) },
      });
      if (result.failed.length) throw new Error(result.failed[0].message);
    });
  }

  const csv = useCsvTransfer({ resource, runChange, onError: setActionError });
  const bulk = useBulkActions({
    resource,
    records,
    rowSelection,
    trashMode,
    actor,
    runChange,
    onError: setActionError,
    confirm: (description, action) =>
      setConfirmation({
        title: trashMode ? "Permanently delete records" : "Delete records",
        description,
        action,
      }),
  });
  const pages = Math.max(1, Math.ceil(total / pageSize));
  const error =
    actionError || (controller.error ? errorMessage(controller.error) : "");
  const busy = controller.changing;
  if (!controller.canList) return <AccessDenied />;
  return (
    <main className="page">
      <div className="page-heading">
        <div>
          <h1>{trashMode ? `${resource.label} trash` : resource.label}</h1>
          <p>{total} records</p>
        </div>
        <div className="toolbar">
          {resource.softDelete && (
            <button
              className="icon-button"
              onClick={() => updateParam("trash", trashMode ? undefined : "1")}
              title={trashMode ? "Show active records" : "Show trash"}
              aria-label={trashMode ? "Show active records" : "Show trash"}
            >
              {trashMode ? <RotateCcw size={17} /> : <Trash2 size={17} />}
            </button>
          )}
          <button
            className="icon-button"
            onClick={csv.exportCsv}
            disabled={csv.exporting}
            title="Export CSV"
            aria-label="Export CSV"
          >
            <Download size={17} />
          </button>
          {canCreate && !trashMode && (
            <>
              <input
                ref={csv.importInput}
                className="sr-only"
                type="file"
                accept=".csv,text/csv"
                onChange={(event) =>
                  void csv.importCsv(event.target.files?.[0])
                }
              />
              <button
                className="icon-button"
                onClick={() => csv.importInput.current?.click()}
                disabled={busy}
                title="Import CSV"
                aria-label="Import CSV"
              >
                <Upload size={17} />
              </button>
            </>
          )}
          <button
            className="icon-button"
            onClick={() => void controller.refetch()}
            disabled={controller.fetching}
            title="Refresh"
            aria-label="Refresh"
          >
            <RefreshCw size={17} />
          </button>
          {canCreate && !trashMode && (
            <Link className="primary-button" to={`/${resource.slug}/new`}>
              <Plus size={17} /> Add
            </Link>
          )}
        </div>
      </div>
      {!trashMode && (
        <div className="list-controls">
          {resource.fields.some(
            (field) =>
              field.searchable &&
              canAccessRule(field.readAccess ?? { mode: "public" }, actor),
          ) && (
            <form className="search-control" onSubmit={submitSearch}>
              <Search size={16} />
              <input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                aria-label="Search"
                placeholder="Search"
              />
            </form>
          )}
          <ResourceFilters
            fields={filterFields}
            resources={resources}
            searchParams={searchParams}
            updateParam={updateParam}
          />
        </div>
      )}
      <SavedViews
        resourceId={resource.id}
        actorId={actor?.id}
        onError={setActionError}
      />
      <ViewOptions
        fields={listFields}
        visibleFieldNames={visibleFields.map((field) => field.name)}
        pageSize={pageSize}
        onColumnsChange={changeColumns}
        onPageSizeChange={(value) => updateParam("page_size", String(value))}
      />
      {!trashMode && (
        <ResourceInsights
          resource={resource}
          fields={filterFields}
          query={{
            q: searchParams.get("q") ?? undefined,
            ...buildResourceFilterQuery(filterFields, searchParams),
          }}
        />
      )}
      <BulkToolbar actions={bulk} trashMode={trashMode} busy={busy} />
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      <ResourceTable
        resource={resource}
        actor={actor}
        records={records}
        visibleFields={visibleFields}
        sort={sort}
        trashMode={trashMode}
        pending={controller.pending}
        fetching={controller.fetching}
        rowSelection={rowSelection}
        setRowSelection={setRowSelection}
        changeSort={changeSort}
        updateField={updateField}
        remove={remove}
        restore={restoreOne}
      />
      <div className="pagination">
        <span>
          Page {page} of {pages}
        </span>
        <div>
          <button
            className="icon-button"
            disabled={page <= 1}
            onClick={() => updateParam("page", String(page - 1))}
            aria-label="Previous page"
          >
            <ChevronLeft size={17} />
          </button>
          <button
            className="icon-button"
            disabled={page >= pages}
            onClick={() => updateParam("page", String(page + 1))}
            aria-label="Next page"
          >
            <ChevronRight size={17} />
          </button>
        </div>
      </div>
      <ConfirmDialog
        open={confirmation !== null}
        title={confirmation?.title ?? "Confirm action"}
        description={confirmation?.description ?? ""}
        confirmLabel="Delete"
        danger
        onCancel={() => setConfirmation(null)}
        onConfirm={async () => {
          const action = confirmation?.action;
          if (action) await action();
          setConfirmation(null);
        }}
      />
    </main>
  );
}

function AccessDenied() {
  return (
    <main className="page">
      <div className="alert" role="alert">
        You do not have permission to view this resource.
      </div>
    </main>
  );
}

function boundedInteger(
  value: string | null,
  minimum: number,
  maximum: number,
  fallback: number,
): number {
  const parsed = Number(value);
  return Number.isInteger(parsed)
    ? Math.min(maximum, Math.max(minimum, parsed))
    : fallback;
}
