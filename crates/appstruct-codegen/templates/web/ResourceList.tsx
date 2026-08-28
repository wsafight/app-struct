import { ArrowDown, ArrowUp, Check, ChevronLeft, ChevronRight, Download, Eye, Plus, RefreshCw, Search, Trash2, Upload } from "lucide-react";
import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import type { FieldDefinition, ResourceDefinition, ResourceRecord } from "../resource";
import { canAccessResource, canAccessRule, errorMessage, useCanAccess, useResourceActor } from "../resource";

export function ResourceList({ resource, resources }: { resource: ResourceDefinition; resources: ResourceDefinition[] }) {
  const actor = useResourceActor();
  const canList = useCanAccess(resource, "list");
  const canCreate = useCanAccess(resource, "create");
  const [searchParams, setSearchParams] = useSearchParams();
  const [records, setRecords] = useState<ResourceRecord[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [search, setSearch] = useState(searchParams.get("q") ?? "");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const writableFields = resource.fields.filter((field) => !field.readOnly && !field.primaryKey && canAccessRule(field.writeAccess ?? { mode: "public" }, actor));
  const [bulkField, setBulkField] = useState(writableFields[0]?.name ?? "");
  const [bulkValue, setBulkValue] = useState("");
  const importInput = useRef<HTMLInputElement>(null);
  const queryKey = searchParams.toString();
  const page = boundedInteger(searchParams.get("page"), 1, Number.MAX_SAFE_INTEGER, 1);
  const pageSize = boundedInteger(searchParams.get("page_size"), 1, 100, 25);
  const sort = searchParams.get("sort") ?? "";
  const columns = useMemo(() => resource.fields.filter((field) => field.kind !== "json" && canAccessRule(field.readAccess ?? { mode: "public" }, actor)).slice(0, 6), [actor, resource]);
  const filterFields = resource.fields.filter((field) => field.filterable && canAccessRule(field.readAccess ?? { mode: "public" }, actor));

  const load = useCallback(async () => {
    if (!canList) return;
    setLoading(true);
    setError("");
    try {
      const exact = Object.fromEntries(filterFields.map((field) => [field.name, searchParams.get(`filter[${field.name}]`) ?? ""]));
      const ranges = Object.fromEntries(filterFields.filter(supportsRange).map((field) => [field.name, {
        gte: searchParams.get(`filter[${field.name}][gte]`) ?? "",
        lte: searchParams.get(`filter[${field.name}][lte]`) ?? "",
      }]));
      const response = await resource.api.list({
        page,
        page_size: pageSize,
        sort: sort || undefined,
        q: searchParams.get("q") ?? undefined,
        filters: exact,
        range_filters: ranges,
      });
      setRecords(response.data);
      setTotal(response.meta.total);
      setSelected(new Set());
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [canList, page, pageSize, queryKey, resource, sort]);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => { setSearch(searchParams.get("q") ?? ""); }, [queryKey]);

  function updateParam(name: string, value?: string) {
    setSearchParams((current) => {
      const next = new URLSearchParams(current);
      if (value) next.set(name, value); else next.delete(name);
      if (name !== "page") next.delete("page");
      return next;
    });
  }

  function submitSearch(event: FormEvent) {
    event.preventDefault();
    updateParam("q", search.trim() || undefined);
  }

  function changeSort(field: string) {
    const next = sort === field ? `-${field}` : sort === `-${field}` ? undefined : field;
    updateParam("sort", next);
  }

  async function remove(id: string) {
    if (!window.confirm(`Delete this ${resource.label}?`)) return;
    try {
      await resource.api.remove(id);
      await load();
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  function toggleSelected(id: string) {
    setSelected((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  }

  function revisionMap(ids: string[]): Record<string, number> {
    return Object.fromEntries(ids.map((id) => [id, Number(records.find((record) => String(record[resource.primaryKey]) === id)?.revision ?? 0)]));
  }

  async function bulkDelete() {
    const ids = [...selected];
    if (!ids.length || !window.confirm(`Delete ${ids.length} selected ${resource.label} records?`)) return;
    try {
      const result = await resource.api.bulkDelete({ ids, expected_revisions: revisionMap(ids) });
      if (result.failed.length) setError(`${result.failed.length} records could not be deleted`);
      await load();
    } catch (reason) { setError(errorMessage(reason)); }
  }

  async function bulkUpdate() {
    const field = resource.fields.find((candidate) => candidate.name === bulkField);
    const ids = [...selected];
    if (!field || !ids.length) return;
    try {
      const result = await resource.api.bulkUpdate({ ids, patch: { [field.name]: inputValue(bulkValue, field) }, expected_revisions: revisionMap(ids) });
      if (result.failed.length) setError(`${result.failed.length} records could not be updated`);
      await load();
    } catch (reason) { setError(errorMessage(reason)); }
  }

  async function exportCsv() {
    try {
      const csv = await resource.api.exportCsv();
      const href = URL.createObjectURL(new Blob([csv], { type: "text/csv;charset=utf-8" }));
      const anchor = document.createElement("a"); anchor.href = href; anchor.download = `${resource.slug}.csv`; anchor.click(); URL.revokeObjectURL(href);
    } catch (reason) { setError(errorMessage(reason)); }
  }

  async function importCsv(file?: File) {
    if (!file) return;
    try {
      const result = await resource.api.importCsv(await file.text());
      if (result.failed.length) setError(`${result.failed.length} rows could not be imported`);
      await load();
    } catch (reason) { setError(errorMessage(reason)); }
  }

  const pages = Math.max(1, Math.ceil(total / pageSize));
  if (!canList) return <AccessDenied />;
  return <main className="page">
    <div className="page-heading"><div><h1>{resource.label}</h1><p>{total} records</p></div><div className="toolbar"><button className="icon-button" onClick={() => void exportCsv()} title="Export CSV" aria-label="Export CSV"><Download size={17} /></button>{canCreate && <><input ref={importInput} className="sr-only" type="file" accept=".csv,text/csv" onChange={(event) => void importCsv(event.target.files?.[0])} /><button className="icon-button" onClick={() => importInput.current?.click()} title="Import CSV" aria-label="Import CSV"><Upload size={17} /></button></>}<button className="icon-button" onClick={() => void load()} title="Refresh" aria-label="Refresh"><RefreshCw size={17} /></button>{canCreate && <Link className="primary-button" to={`/${resource.slug}/new`}><Plus size={17} /> Add</Link>}</div></div>
    <div className="list-controls">
      {resource.fields.some((field) => field.searchable && canAccessRule(field.readAccess ?? { mode: "public" }, actor)) && <form className="search-control" onSubmit={submitSearch}><Search size={16} /><input value={search} onChange={(event) => setSearch(event.target.value)} aria-label="Search" placeholder="Search" /></form>}
      {filterFields.map((field) => <FilterControl key={field.name} field={field} resources={resources} searchParams={searchParams} updateParam={updateParam} />)}
    </div>
    {selected.size > 0 && <div className="bulk-toolbar"><strong>{selected.size} selected</strong>{writableFields.length > 0 && <><select aria-label="Field to update" value={bulkField} onChange={(event) => setBulkField(event.target.value)}>{writableFields.map((field) => <option key={field.name} value={field.name}>{field.label}</option>)}</select><input aria-label="Bulk value" value={bulkValue} onChange={(event) => setBulkValue(event.target.value)} /><button className="secondary-button" onClick={() => void bulkUpdate()}><Check size={16} /> Apply</button></>}<button className="icon-button danger" onClick={() => void bulkDelete()} title="Delete selected" aria-label="Delete selected"><Trash2 size={16} /></button></div>}
    {error && <div className="alert" role="alert">{error}</div>}
    <div className="table-frame"><table><thead><tr><th className="selection-cell"><input type="checkbox" aria-label="Select page" checked={records.length > 0 && records.every((record) => selected.has(String(record[resource.primaryKey])))} onChange={(event) => setSelected(event.target.checked ? new Set(records.map((record) => String(record[resource.primaryKey]))) : new Set())} /></th>{columns.map((field) => <th key={field.name}>{field.sortable || field.primaryKey ? <button className="sort-button" onClick={() => changeSort(field.name)}>{field.label}{sort === field.name ? <ArrowUp size={14} /> : sort === `-${field.name}` ? <ArrowDown size={14} /> : null}</button> : field.label}</th>)}<th><span className="sr-only">Actions</span></th></tr></thead><tbody>
      {loading && <tr><td colSpan={columns.length + 2} className="empty">Loading...</td></tr>}
      {!loading && records.length === 0 && <tr><td colSpan={columns.length + 2} className="empty">No records</td></tr>}
      {!loading && records.map((record) => { const id = String(record[resource.primaryKey]); const canRead = canAccessResource(resource, "read", actor, record); const canDelete = canAccessResource(resource, "delete", actor, record); return <tr key={id}><td className="selection-cell"><input type="checkbox" checked={selected.has(id)} onChange={() => toggleSelected(id)} aria-label={`Select ${id}`} /></td>{columns.map((field) => <td key={field.name}>{formatValue(record[field.name])}</td>)}<td className="row-actions">{canRead && <Link className="icon-button" to={`/${resource.slug}/${encodeURIComponent(id)}`} title="View" aria-label="View"><Eye size={16} /></Link>}{canDelete && <button className="icon-button danger" onClick={() => void remove(id)} title="Delete" aria-label="Delete"><Trash2 size={16} /></button>}</td></tr>; })}
    </tbody></table></div>
    <div className="pagination"><span>Page {page} of {pages}</span><div><button className="icon-button" disabled={page <= 1} onClick={() => updateParam("page", String(page - 1))} aria-label="Previous page"><ChevronLeft size={17} /></button><button className="icon-button" disabled={page >= pages} onClick={() => updateParam("page", String(page + 1))} aria-label="Next page"><ChevronRight size={17} /></button></div></div>
  </main>;
}

function FilterControl({ field, resources, searchParams, updateParam }: { field: FieldDefinition; resources: ResourceDefinition[]; searchParams: URLSearchParams; updateParam(name: string, value?: string): void }) {
  if (supportsRange(field)) {
    return <label className="filter-control"><span>{field.label}</span><span className="range-filter">
      {(["gte", "lte"] as const).map((operator) => { const name = `filter[${field.name}][${operator}]`; return <input key={operator} type={filterInputType(field.kind)} aria-label={`${field.label} ${operator === "gte" ? "from" : "to"}`} placeholder={operator === "gte" ? "From" : "To"} value={filterDisplayValue(searchParams.get(name) ?? "", field.kind)} onChange={(event) => updateParam(name, filterApiValue(event.target.value, field.kind))} />; })}
    </span></label>;
  }
  if (field.kind === "relation") {
    return <RelationFilter field={field} target={resources.find((resource) => resource.id === field.relation)} value={searchParams.get(`filter[${field.name}]`) ?? ""} onChange={(value) => updateParam(`filter[${field.name}]`, value)} />;
  }
  const name = `filter[${field.name}]`;
  if (field.kind === "enum" || field.kind === "boolean") {
    const values = field.kind === "boolean" ? ["true", "false"] : field.values ?? [];
    return <label className="filter-control"><span>{field.label}</span><select value={searchParams.get(name) ?? ""} onChange={(event) => updateParam(name, event.target.value || undefined)}><option value="">All</option>{values.map((value) => <option key={value} value={value}>{field.kind === "boolean" ? value === "true" ? "Yes" : "No" : value}</option>)}</select></label>;
  }
  return <label className="filter-control"><span>{field.label}</span><input value={searchParams.get(name) ?? ""} onChange={(event) => updateParam(name, event.target.value || undefined)} /></label>;
}

function RelationFilter({ field, target, value, onChange }: { field: FieldDefinition; target?: ResourceDefinition; value: string; onChange(value?: string): void }) {
  const actor = useResourceActor();
  const [options, setOptions] = useState<ResourceRecord[]>([]);
  useEffect(() => {
    if (!target || !canAccessResource(target, "list", actor)) return;
    target.api.list({ page_size: 100 }).then((response) => setOptions(response.data)).catch(() => setOptions([]));
  }, [actor, target]);
  const labelField = target?.fields.find((item) => !item.primaryKey && (item.kind === "string" || item.kind === "text"));
  return <label className="filter-control"><span>{field.label}</span><select value={value} onChange={(event) => onChange(event.target.value || undefined)}><option value="">All</option>{options.map((record) => { const optionValue = String(record[target?.primaryKey ?? "id"]); return <option key={optionValue} value={optionValue}>{String(record[labelField?.name ?? target?.primaryKey ?? "id"])}</option>; })}</select></label>;
}

function AccessDenied() {
  return <main className="page"><div className="alert" role="alert">You do not have permission to view this resource.</div></main>;
}

function supportsRange(field: FieldDefinition): boolean {
  return ["integer", "bigint", "decimal", "date", "datetime"].includes(field.kind);
}

function filterInputType(kind: FieldDefinition["kind"]): string {
  if (kind === "date") return "date";
  if (kind === "datetime") return "datetime-local";
  return "number";
}

function filterDisplayValue(value: string, kind: FieldDefinition["kind"]): string {
  return kind === "datetime" ? value.slice(0, 16) : value;
}

function filterApiValue(value: string, kind: FieldDefinition["kind"]): string | undefined {
  if (!value) return undefined;
  return kind === "datetime" ? new Date(value).toISOString() : value;
}

function boundedInteger(value: string | null, minimum: number, maximum: number, fallback: number): number {
  const parsed = Number(value);
  return Number.isInteger(parsed) ? Math.min(maximum, Math.max(minimum, parsed)) : fallback;
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
