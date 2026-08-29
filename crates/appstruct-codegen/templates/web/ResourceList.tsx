import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createColumnHelper,
  rowSelectionFeature,
  tableFeatures,
  useTable,
  type RowSelectionState,
} from "@tanstack/react-table";
import { ArrowDown, ArrowUp, Bookmark, Check, ChevronLeft, ChevronRight, Copy, Download, Eye, Plus, RefreshCw, RotateCcw, Search, Trash2, Upload } from "lucide-react";
import { type FormEvent, type InputHTMLAttributes, useEffect, useMemo, useRef, useState } from "react";
import { Link, useSearchParams } from "../navigation";
import { resourceQueryKeys } from "../query";
import type { FieldDefinition, ResourceDefinition, ResourceRecord } from "../resource";
import { canAccessResource, canAccessRule, errorMessage, useCanAccess, useResourceActor } from "../resource";
import { buildResourceFilterQuery, ResourceFilters } from "./ResourceFilters";

interface SavedView {
  id: string;
  name: string;
  query: string;
}

interface ListResult {
  data: ResourceRecord[];
  total: number;
}

const EMPTY_LIST_RESULT: ListResult = { data: [], total: 0 };
const resourceTableFeatures = tableFeatures({ rowSelectionFeature });
const resourceColumnHelper = createColumnHelper<typeof resourceTableFeatures, ResourceRecord>();

export function ResourceList({ resource, resources }: { resource: ResourceDefinition; resources: ResourceDefinition[] }) {
  const actor = useResourceActor();
  const canList = useCanAccess(resource, "list");
  const canCreate = useCanAccess(resource, "create");
  const queryClient = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const [search, setSearch] = useState(searchParams.get("q") ?? "");
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [actionError, setActionError] = useState("");
  const writableFields = useMemo(
    () => resource.fields.filter((field) => !field.readOnly && !field.primaryKey && canAccessRule(field.writeAccess ?? { mode: "public" }, actor)),
    [actor, resource],
  );
  const [bulkField, setBulkField] = useState(writableFields[0]?.name ?? "");
  const [bulkValue, setBulkValue] = useState("");
  const [savedViews, setSavedViews] = useState<SavedView[]>([]);
  const [viewName, setViewName] = useState("");
  const importInput = useRef<HTMLInputElement>(null);
  const viewStorageKey = `appstruct.saved-views.${resource.id}.${actor?.id ?? "anonymous"}.${tenantStorageScope()}`;
  const queryString = searchParams.toString();
  const trashMode = resource.softDelete && searchParams.get("trash") === "1";
  const page = boundedInteger(searchParams.get("page"), 1, Number.MAX_SAFE_INTEGER, 1);
  const pageSize = boundedInteger(searchParams.get("page_size"), 1, 100, 25);
  const sort = searchParams.get("sort") ?? "";
  const visibleFields = useMemo(
    () => resource.fields.filter((field) => field.kind !== "json" && canAccessRule(field.readAccess ?? { mode: "public" }, actor)).slice(0, 6),
    [actor, resource],
  );
  const filterFields = useMemo(
    () => resource.fields.filter((field) => field.filterable && canAccessRule(field.readAccess ?? { mode: "public" }, actor)),
    [actor, resource],
  );

  const listQuery = useQuery({
    queryKey: resourceQueryKeys.list(resource.id, `${trashMode ? "trash" : "active"}:${queryString}`),
    queryFn: async ({ signal }): Promise<ListResult> => {
      if (trashMode) {
        const response = await resource.api.trash?.({ page, page_size: pageSize }, { signal });
        return { data: response?.data ?? [], total: response?.meta.total ?? 0 };
      }
      const response = await resource.api.list({
        page,
        page_size: pageSize,
        sort: sort || undefined,
        q: searchParams.get("q") ?? undefined,
        ...buildResourceFilterQuery(filterFields, searchParams),
      }, { signal });
      return { data: response.data, total: response.meta.total };
    },
    enabled: canList,
    placeholderData: (previous) => previous,
  });
  const { data: records, total } = listQuery.data ?? EMPTY_LIST_RESULT;

  const changeMutation = useMutation({
    mutationFn: (operation: () => Promise<void>) => operation(),
    onSuccess: async () => {
      setRowSelection({});
      await queryClient.invalidateQueries({ queryKey: resourceQueryKeys.all(resource.id) });
    },
  });
  const exportMutation = useMutation({
    mutationFn: () => resource.api.exportCsv(),
    onSuccess: (csv) => {
      const href = URL.createObjectURL(new Blob([csv], { type: "text/csv;charset=utf-8" }));
      const anchor = document.createElement("a");
      anchor.href = href;
      anchor.download = `${resource.slug}.csv`;
      anchor.click();
      URL.revokeObjectURL(href);
    },
    onError: (reason) => setActionError(errorMessage(reason)),
  });

  useEffect(() => {
    setSearch(new URLSearchParams(queryString).get("q") ?? "");
    setActionError("");
    setRowSelection({});
  }, [queryString]);
  useEffect(() => { setRowSelection({}); }, [listQuery.dataUpdatedAt]);
  useEffect(() => {
    if (!canList) return;
    let active = true;
    let source: EventSource | undefined;
    const refresh = () => { void queryClient.invalidateQueries({ queryKey: resourceQueryKeys.all(resource.id) }); };
    void import("../generated/client").then((module) => {
      if (!active) return;
      const subscribe = (module as { subscribeRealtime?: (scope?: { resource?: string }) => EventSource }).subscribeRealtime;
      if (!subscribe) return;
      source = subscribe({ resource: resource.slug });
      for (const event of [`${resource.eventPrefix}.created`, `${resource.eventPrefix}.updated`, `${resource.eventPrefix}.deleted`, "resync"]) {
        source.addEventListener(event, refresh);
      }
    }).catch(() => undefined);
    return () => { active = false; source?.close(); };
  }, [canList, queryClient, resource.eventPrefix, resource.id, resource.slug]);
  useEffect(() => {
    try {
      setSavedViews(JSON.parse(localStorage.getItem(viewStorageKey) ?? "[]") as SavedView[]);
    } catch {
      setSavedViews([]);
    }
  }, [viewStorageKey]);

  function updateParam(name: string, value?: string, replace = false) {
    setSearchParams((current) => {
      const next = new URLSearchParams(current);
      if (value) next.set(name, value); else next.delete(name);
      if (name !== "page") next.delete("page");
      return next;
    }, { replace });
  }

  function submitSearch(event: FormEvent) {
    event.preventDefault();
    updateParam("q", search.trim() || undefined);
  }

  function changeSort(field: string) {
    const next = sort === field ? `-${field}` : sort === `-${field}` ? undefined : field;
    updateParam("sort", next);
  }

  async function runChange(operation: () => Promise<void>) {
    setActionError("");
    try {
      await changeMutation.mutateAsync(operation);
    } catch (reason) {
      setActionError(errorMessage(reason));
    }
  }

  async function remove(id: string) {
    const action = resource.softDelete ? "Move this record to trash" : "Delete this record";
    if (!window.confirm(`${action}?`)) return;
    await runChange(() => resource.api.remove(id));
  }

  function selectedIds(): string[] {
    return Object.keys(rowSelection);
  }

  function revisionMap(ids: string[]): Record<string, number> {
    return Object.fromEntries(ids.map((id) => [id, Number(records.find((record) => String(record[resource.primaryKey]) === id)?.revision ?? 0)]));
  }

  async function bulkDelete() {
    const ids = selectedIds();
    if (!ids.length || !window.confirm(`${trashMode ? "Permanently delete" : resource.softDelete ? "Move to trash" : "Delete"} ${ids.length} selected ${resource.label} records?`)) return;
    await runChange(async () => {
      const result = await resource.api.bulkDelete({ ids, expected_revisions: revisionMap(ids) });
      if (result.failed.length) setActionError(`${result.failed.length} records could not be deleted`);
    });
  }

  async function restoreSelected() {
    const ids = selectedIds();
    if (!ids.length || !resource.api.restore) return;
    await runChange(async () => {
      const result = await resource.api.restore!({ ids, expected_revisions: revisionMap(ids) });
      if (result.failed.length) setActionError(`${result.failed.length} records could not be restored`);
    });
  }

  async function restoreOne(id: string) {
    if (!resource.api.restore) return;
    await runChange(async () => {
      const result = await resource.api.restore!({ ids: [id], expected_revisions: revisionMap([id]) });
      if (result.failed.length) setActionError(result.failed[0].message);
    });
  }

  async function bulkUpdate() {
    const field = resource.fields.find((candidate) => candidate.name === bulkField);
    const ids = selectedIds();
    if (!field || !ids.length) return;
    await runChange(async () => {
      const result = await resource.api.bulkUpdate({ ids, patch: { [field.name]: inputValue(bulkValue, field) }, expected_revisions: revisionMap(ids) });
      if (result.failed.length) setActionError(`${result.failed.length} records could not be updated`);
    });
  }

  async function importCsv(file?: File) {
    if (!file) return;
    await runChange(async () => {
      const result = await resource.api.importCsv(await file.text());
      if (result.failed.length) setActionError(`${result.failed.length} rows could not be imported`);
    });
    if (importInput.current) importInput.current.value = "";
  }

  function saveView() {
    const name = viewName.trim();
    if (!name) return;
    const view = { id: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`, name, query: searchParams.toString() };
    const next = [...savedViews.filter((item) => item.name !== name), view];
    setSavedViews(next);
    setViewName("");
    localStorage.setItem(viewStorageKey, JSON.stringify(next));
  }

  function applyView(view: SavedView) {
    setSearchParams(new URLSearchParams(view.query));
  }

  function deleteView(view: SavedView) {
    const next = savedViews.filter((item) => item.id !== view.id);
    setSavedViews(next);
    localStorage.setItem(viewStorageKey, JSON.stringify(next));
  }

  async function copyViewLink() {
    try {
      await navigator.clipboard.writeText(window.location.href);
      setActionError("");
    } catch {
      setActionError("Could not copy view link");
    }
  }

  const tableColumns = [
    resourceColumnHelper.display({
      id: "selection",
      header: ({ table }) => <SelectionCheckbox aria-label="Select page" checked={table.getIsAllPageRowsSelected()} indeterminate={table.getIsSomePageRowsSelected() && !table.getIsAllPageRowsSelected()} onChange={table.getToggleAllPageRowsSelectedHandler()} />,
      cell: ({ row }) => <input type="checkbox" checked={row.getIsSelected()} onChange={row.getToggleSelectedHandler()} aria-label={`Select ${row.id}`} />,
    }),
    ...visibleFields.map((field) => resourceColumnHelper.accessor((record) => record[field.name], {
      id: field.name,
      header: () => field.sortable || field.primaryKey ? <button className="sort-button" onClick={() => changeSort(field.name)}>{field.label}{sort === field.name ? <ArrowUp size={14} /> : sort === `-${field.name}` ? <ArrowDown size={14} /> : null}</button> : field.label,
      cell: (info) => formatValue(info.getValue()),
    })),
    resourceColumnHelper.display({
      id: "actions",
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) => {
        const record = row.original;
        const id = String(record[resource.primaryKey]);
        const canRead = canAccessResource(resource, "read", actor, record);
        const canDelete = canAccessResource(resource, "delete", actor, record);
        return <div className="row-actions">{canRead && <Link className="icon-button" to={`/${resource.slug}/${encodeURIComponent(id)}`} title="View" aria-label="View"><Eye size={16} /></Link>}{trashMode ? <button className="icon-button" onClick={() => void restoreOne(id)} title="Restore" aria-label="Restore"><RotateCcw size={16} /></button> : canDelete && <button className="icon-button danger" onClick={() => void remove(id)} title={resource.softDelete ? "Move to trash" : "Delete"} aria-label={resource.softDelete ? "Move to trash" : "Delete"}><Trash2 size={16} /></button>}</div>;
      },
    }),
  ];

  const table = useTable({
    features: resourceTableFeatures,
    columns: tableColumns,
    data: records,
    getRowId: (record) => String(record[resource.primaryKey]),
    state: { rowSelection },
    onRowSelectionChange: setRowSelection,
  });

  const pages = Math.max(1, Math.ceil(total / pageSize));
  const error = actionError || (listQuery.error ? errorMessage(listQuery.error) : "");
  const busy = changeMutation.isPending;
  if (!canList) return <AccessDenied />;
  return <main className="page">
    <div className="page-heading"><div><h1>{trashMode ? `${resource.label} trash` : resource.label}</h1><p>{total} records</p></div><div className="toolbar">{resource.softDelete && <button className="icon-button" onClick={() => updateParam("trash", trashMode ? undefined : "1")} title={trashMode ? "Show active records" : "Show trash"} aria-label={trashMode ? "Show active records" : "Show trash"}>{trashMode ? <RotateCcw size={17} /> : <Trash2 size={17} />}</button>}<button className="icon-button" onClick={() => exportMutation.mutate()} disabled={exportMutation.isPending} title="Export CSV" aria-label="Export CSV"><Download size={17} /></button>{canCreate && !trashMode && <><input ref={importInput} className="sr-only" type="file" accept=".csv,text/csv" onChange={(event) => void importCsv(event.target.files?.[0])} /><button className="icon-button" onClick={() => importInput.current?.click()} disabled={busy} title="Import CSV" aria-label="Import CSV"><Upload size={17} /></button></>}<button className="icon-button" onClick={() => void listQuery.refetch()} disabled={listQuery.isFetching} title="Refresh" aria-label="Refresh"><RefreshCw size={17} /></button>{canCreate && !trashMode && <Link className="primary-button" to={`/${resource.slug}/new`}><Plus size={17} /> Add</Link>}</div></div>
    {!trashMode && <div className="list-controls">
      {resource.fields.some((field) => field.searchable && canAccessRule(field.readAccess ?? { mode: "public" }, actor)) && <form className="search-control" onSubmit={submitSearch}><Search size={16} /><input value={search} onChange={(event) => setSearch(event.target.value)} aria-label="Search" placeholder="Search" /></form>}
      <ResourceFilters fields={filterFields} resources={resources} searchParams={searchParams} updateParam={updateParam} />
    </div>}
    <div className="view-toolbar"><Bookmark size={16} /><select aria-label="Saved views" value="" onChange={(event) => { const view = savedViews.find((item) => item.id === event.target.value); if (view) applyView(view); }}><option value="">Saved views</option>{savedViews.map((view) => <option key={view.id} value={view.id}>{view.name}</option>)}</select><input aria-label="View name" placeholder="Name this view" value={viewName} onChange={(event) => setViewName(event.target.value)} /><button className="secondary-button" onClick={saveView} disabled={!viewName.trim()}>Save</button><button className="icon-button" onClick={() => void copyViewLink()} title="Copy share link" aria-label="Copy share link"><Copy size={16} /></button>{savedViews.length > 0 && <button className="icon-button danger" onClick={() => { const view = savedViews.at(-1); if (view) deleteView(view); }} title="Delete last saved view" aria-label="Delete last saved view"><Trash2 size={16} /></button>}</div>
    {selectedIds().length > 0 && <div className="bulk-toolbar"><strong>{selectedIds().length} selected</strong>{!trashMode && writableFields.length > 0 && <><select aria-label="Field to update" value={bulkField} onChange={(event) => setBulkField(event.target.value)}>{writableFields.map((field) => <option key={field.name} value={field.name}>{field.label}</option>)}</select><input aria-label="Bulk value" value={bulkValue} onChange={(event) => setBulkValue(event.target.value)} /><button className="secondary-button" disabled={busy} onClick={() => void bulkUpdate()}><Check size={16} /> Apply</button></>}{trashMode ? <button className="secondary-button" disabled={busy} onClick={() => void restoreSelected()}><RotateCcw size={16} /> Restore</button> : <button className="icon-button danger" disabled={busy} onClick={() => void bulkDelete()} title="Delete selected" aria-label="Delete selected"><Trash2 size={16} /></button>}</div>}
    {error && <div className="alert" role="alert">{error}</div>}
    <div className="table-frame"><table aria-busy={listQuery.isFetching}><thead>{table.getHeaderGroups().map((headerGroup) => <tr key={headerGroup.id}>{headerGroup.headers.map((header) => <th key={header.id} className={header.column.id === "selection" ? "selection-cell" : undefined}>{header.isPlaceholder ? null : <table.FlexRender header={header} />}</th>)}</tr>)}</thead><tbody>
      {listQuery.isPending && <tr><td colSpan={table.getAllLeafColumns().length} className="empty">Loading...</td></tr>}
      {!listQuery.isPending && table.getRowModel().rows.length === 0 && <tr><td colSpan={table.getAllLeafColumns().length} className="empty">No records</td></tr>}
      {!listQuery.isPending && table.getRowModel().rows.map((row) => <tr key={row.id}>{row.getAllCells().map((cell) => <td key={cell.id} className={cell.column.id === "selection" ? "selection-cell" : cell.column.id === "actions" ? "row-actions-cell" : undefined}><table.FlexRender cell={cell} /></td>)}</tr>)}
    </tbody></table></div>
    <div className="pagination"><span>Page {page} of {pages}</span><div><button className="icon-button" disabled={page <= 1} onClick={() => updateParam("page", String(page - 1))} aria-label="Previous page"><ChevronLeft size={17} /></button><button className="icon-button" disabled={page >= pages} onClick={() => updateParam("page", String(page + 1))} aria-label="Next page"><ChevronRight size={17} /></button></div></div>
  </main>;
}

function SelectionCheckbox({ indeterminate, ...props }: InputHTMLAttributes<HTMLInputElement> & { indeterminate?: boolean }) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => { if (ref.current) ref.current.indeterminate = Boolean(indeterminate); }, [indeterminate]);
  return <input ref={ref} type="checkbox" {...props} />;
}

function AccessDenied() {
  return <main className="page"><div className="alert" role="alert">You do not have permission to view this resource.</div></main>;
}

function boundedInteger(value: string | null, minimum: number, maximum: number, fallback: number): number {
  const parsed = Number(value);
  return Number.isInteger(parsed) ? Math.min(maximum, Math.max(minimum, parsed)) : fallback;
}

function tenantStorageScope(): string {
  try {
    return window.localStorage.getItem("appstruct_tenant") ?? "global";
  } catch {
    return "global";
  }
}

export function formatValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function inputValue(value: string, field: FieldDefinition): unknown {
  if (field.kind === "boolean") return value === "true";
  if (["integer", "bigint"].includes(field.kind)) return Number(value);
  return value;
}
