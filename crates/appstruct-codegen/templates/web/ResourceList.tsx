import { Edit3, Plus, RefreshCw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import type { ResourceDefinition, ResourceRecord } from "../resource";
import { errorMessage } from "../resource";

export function ResourceList({ resource }: { resource: ResourceDefinition }) {
  const [records, setRecords] = useState<ResourceRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      setRecords(await resource.api.list());
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [resource]);

  useEffect(() => { void load(); }, [load]);

  async function remove(id: string) {
    if (!window.confirm(`Delete this ${resource.label}?`)) return;
    try {
      await resource.api.remove(id);
      await load();
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  const columns = resource.fields.filter((field) => field.kind !== "json").slice(0, 6);
  return (
    <main className="page">
      <div className="page-heading">
        <div><h1>{resource.label}</h1><p>{records.length} records</p></div>
        <div className="toolbar">
          <button className="icon-button" onClick={() => void load()} title="Refresh" aria-label="Refresh">
            <RefreshCw size={17} />
          </button>
          <Link className="primary-button" to={`/${resource.slug}/new`}><Plus size={17} /> Add</Link>
        </div>
      </div>
      {error && <div className="alert" role="alert">{error}</div>}
      <div className="table-frame">
        <table>
          <thead><tr>{columns.map((field) => <th key={field.name}>{field.label}</th>)}<th><span className="sr-only">Actions</span></th></tr></thead>
          <tbody>
            {loading && <tr><td colSpan={columns.length + 1} className="empty">Loading...</td></tr>}
            {!loading && records.length === 0 && <tr><td colSpan={columns.length + 1} className="empty">No records</td></tr>}
            {!loading && records.map((record) => {
              const id = String(record[resource.primaryKey]);
              return <tr key={id}>
                {columns.map((field) => <td key={field.name}>{formatValue(record[field.name])}</td>)}
                <td className="row-actions">
                  <Link className="icon-button" to={`/${resource.slug}/${encodeURIComponent(id)}/edit`} title="Edit" aria-label="Edit"><Edit3 size={16} /></Link>
                  <button className="icon-button danger" onClick={() => void remove(id)} title="Delete" aria-label="Delete"><Trash2 size={16} /></button>
                </td>
              </tr>;
            })}
          </tbody>
        </table>
      </div>
    </main>
  );
}

function formatValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

