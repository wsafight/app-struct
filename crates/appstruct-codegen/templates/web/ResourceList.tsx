import { formatNumber, t } from "../ui";
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
import { AsyncState } from "../components/AsyncState";
import { ConfirmDialog } from "../components/Dialog";
import {
  useResourceListController,
  useResourceUrlController,
} from "../controller";
import { Link } from "../navigation";
import type {
  FieldDefinition,
  ResourceDefinition,
  ResourceRecord,
} from "../resource";
import {
  canAccessRule,
  errorMessage,
  listProjectionFields,
  useCanAccess,
  useResourceActor,
} from "../resource";
import { useRealtimeResource } from "../realtime/useRealtimeResource";
import { ResourceFilters } from "./ResourceFilters";
import { buildResourceFilterQuery } from "../url-controller";
import { BulkToolbar, useBulkActions } from "./resource-list/BulkActions";
import { ResourceInsights } from "./resource-list/ResourceInsights";
import { revisionMap } from "./resource-list/revisions";
import { ResourceTable } from "./resource-list/ResourceTable";
import { SavedViews } from "./resource-list/SavedViews";
import { useCsvTransfer } from "./resource-list/useCsvTransfer";
import { CsvImportDialog } from "./resource-list/CsvImportDialog";
import {
  defaultVisibleFieldNames,
  resolveVisibleFields,
  ViewOptions,
} from "./resource-list/ViewOptions";

export { formatFieldValue, formatValue } from "./resource-list/ResourceTable";

export function ResourceList({
  resource,
  resources,
}: {
  resource: ResourceDefinition;
  resources: ResourceDefinition[];
}) {
  const actor = useResourceActor();
  const canCreate = useCanAccess(resource, "create");
  const url = useResourceUrlController(resource);
  const {
    searchParams,
    trashMode,
    page,
    pageSize,
    sort,
    cursorMode,
    filterFields,
    updateParam,
    moveCursor,
  } = url;
  const [search, setSearch] = useState(searchParams.get("q") ?? "");
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [actionError, setActionError] = useState("");
  const [confirmation, setConfirmation] = useState<{
    title: string;
    description: string;
    action(): Promise<void>;
  } | null>(null);
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
  const listCacheKey = listQueryKey(searchParams);
  const controller = useResourceListController(resource, {
    cacheKey: listCacheKey,
    trashMode,
    cursorMode,
    query: {
      ...url.query,
      fields: listProjectionFields(resource, visibleFields, actor),
    },
    onChangeSuccess: () => setRowSelection({}),
  });
  const { records, total, nextCursor, previousCursor } = controller;

  useEffect(() => {
    setSearch(new URLSearchParams(listCacheKey).get("q") ?? "");
    setActionError("");
    setRowSelection({});
  }, [listCacheKey]);
  useEffect(() => {
    setRowSelection({});
  }, [controller.dataUpdatedAt]);
  useRealtimeResource({
    enabled: controller.canList,
    resourceId: resource.id,
    resourceSlug: resource.slug,
    eventPrefix: resource.eventPrefix,
  });

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
      ? t("Move this record to trash")
      : t("Delete this record");
    setConfirmation({
      title: action,
      description: t("This action cannot be undone from the current view."),
      action: async () => {
        await runChange(() => resource.api.remove(id));
      },
    });
  }

  async function restoreOne(id: string) {
    if (!resource.api.restore) return;
    await runChange(async () => {
      const result = await resource.api.restore!({
        ids: [id],
        expected_revisions: revisionMap(records, resource.primaryKey, [id]),
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

  const csv = useCsvTransfer({
    resource,
    runChange,
    onError: setActionError,
    query: {
      ...buildResourceFilterQuery(filterFields, searchParams),
      q: searchParams.get("q") || undefined,
      sort: sort || undefined,
      fields: resource.fields
        .filter(
          (field) =>
            visibleFields.some(
              (visible) =>
                visible.name === field.name ||
                visible.semantic?.currencyField === field.name,
            ) && canAccessRule(field.readAccess ?? { mode: "public" }, actor),
        )
        .map((field) => field.name),
    },
  });
  const bulk = useBulkActions({
    resource,
    records,
    rowSelection,
    trashMode,
    actor,
    runChange,
    onError: setActionError,
  });
  const pages = Math.max(1, Math.ceil(total / pageSize));
  const error =
    actionError ||
    (controller.error && controller.error !== controller.loadError
      ? errorMessage(controller.error)
      : "");
  const hasFilters =
    Boolean(searchParams.get("q")?.trim()) ||
    Boolean(searchParams.get("where")) ||
    Array.from(searchParams.keys()).some((key) => key.startsWith("filter["));
  const emptyMessage = trashMode
    ? t("Trash is empty")
    : hasFilters
      ? t("No records match the current filters")
      : t("No records yet");
  const busy = controller.changing;
  if (!controller.canList) return <AccessDenied />;
  return (
    <main className="page resource-list-page">
      {csv.importFile && (
        <CsvImportDialog
          resource={resource}
          file={csv.importFile}
          runChange={runChange}
          onError={setActionError}
          onClose={csv.closeImport}
        />
      )}
      <div className="page-heading">
        <div>
          <h1>
            {trashMode ? t("{0} trash", { 0: resource.label }) : resource.label}
          </h1>
          <p>
            {cursorMode
              ? t("{0} records on this page", {
                  0: formatNumber(records.length),
                })
              : t("{0} records", { 0: formatNumber(total) })}
          </p>
        </div>
        <div className="toolbar">
          {resource.softDelete && (
            <button
              className="icon-button"
              onClick={() => updateParam("trash", trashMode ? undefined : "1")}
              title={trashMode ? t("Show active records") : t("Show trash")}
              aria-label={
                trashMode ? t("Show active records") : t("Show trash")
              }
            >
              {trashMode ? <RotateCcw size={17} /> : <Trash2 size={17} />}
            </button>
          )}
          <button
            className="icon-button"
            onClick={csv.exportCsv}
            disabled={csv.exporting}
            title={t("Export CSV")}
            aria-label={t("Export CSV")}
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
                title={t("Import CSV")}
                aria-label={t("Import CSV")}
              >
                <Upload size={17} />
              </button>
            </>
          )}
          <button
            className="icon-button"
            onClick={() => void controller.refetch()}
            disabled={controller.fetching}
            title={t("Refresh")}
            aria-label={t("Refresh")}
          >
            <RefreshCw size={17} />
          </button>
          {canCreate && !trashMode && (
            <Link className="primary-button" to={`/${resource.slug}/new`}>
              <Plus size={17} /> {t("Add")}
            </Link>
          )}
        </div>
      </div>
      <section className="resource-controls" aria-label={t("List controls")}>
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
                  aria-label={t("Search")}
                  placeholder={t("Search")}
                />
              </form>
            )}
            <ResourceFilters
              resource={resource}
              fields={filterFields}
              resources={resources}
              searchParams={searchParams}
              updateParam={updateParam}
              updateParams={url.updateParams}
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
      </section>
      {!trashMode && (
        <ResourceInsights
          resource={resource}
          resources={resources}
          fields={filterFields}
          parameters={searchParams}
          updateParams={(values) => url.updateParams(values, false)}
          query={{
            q: searchParams.get("q") ?? undefined,
            ...buildResourceFilterQuery(filterFields, searchParams),
          }}
        />
      )}
      <BulkToolbar
        actions={bulk}
        trashMode={trashMode}
        busy={busy}
        resources={resources}
      />
      {controller.loadError && records.length > 0 && (
        <AsyncState
          state="error"
          message={errorMessage(controller.loadError)}
          onRetry={() => void controller.refetch()}
        />
      )}
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      <ResourceTable
        resources={resources}
        resource={resource}
        actor={actor}
        records={records}
        visibleFields={visibleFields}
        sort={sort}
        trashMode={trashMode}
        pending={controller.pending}
        fetching={controller.fetching}
        loadError={controller.loadError}
        emptyMessage={emptyMessage}
        onRetry={() => void controller.refetch()}
        rowSelection={rowSelection}
        setRowSelection={setRowSelection}
        changeSort={changeSort}
        updateField={updateField}
        remove={remove}
        restore={restoreOne}
      />
      <div className="pagination">
        <span>
          {cursorMode
            ? t("Page {0}", { "0": page })
            : t("Page {0} of {1}", { "0": page, "1": pages })}
        </span>
        <div>
          <button
            className="icon-button"
            disabled={cursorMode ? !previousCursor : page <= 1}
            onClick={() =>
              cursorMode && previousCursor
                ? moveCursor(previousCursor, "previous", page - 1)
                : updateParam("page", String(page - 1))
            }
            aria-label={t("Previous page")}
          >
            <ChevronLeft size={17} />
          </button>
          <button
            className="icon-button"
            disabled={cursorMode ? !nextCursor : page >= pages}
            onClick={() =>
              cursorMode && nextCursor
                ? moveCursor(nextCursor, "next", page + 1)
                : updateParam("page", String(page + 1))
            }
            aria-label={t("Next page")}
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

function listQueryKey(parameters: URLSearchParams): string {
  const list = new URLSearchParams(parameters);
  for (const key of Array.from(list.keys()))
    if (key === "summary" || key.startsWith("report_")) list.delete(key);
  return list.toString();
}

function AccessDenied() {
  return (
    <main className="page">
      <div className="alert" role="alert">
        {" "}
        {t("You do not have permission to view this resource.")}{" "}
      </div>
    </main>
  );
}
