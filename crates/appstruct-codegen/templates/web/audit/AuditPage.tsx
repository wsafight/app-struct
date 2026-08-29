import { ChevronLeft, ChevronRight, History } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { auditApi, type AuditEvent } from "../generated/client";
import { auditAccess } from "../generated/resources";
import { errorMessage, useCanAccessRule } from "../resource";

export function AuditPage() {
  const canRead = useCanAccessRule(auditAccess);
  const [events, setEvents] = useState<AuditEvent[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const requestId = useRef(0);
  const pageSize = 25;

  useEffect(() => {
    if (!canRead) return;
    const currentRequest = ++requestId.current;
    let active = true;
    setLoading(true);
    setError("");
    auditApi.list({ page, page_size: pageSize })
      .then((response) => { if (active && currentRequest === requestId.current) { setEvents(response.data); setTotal(response.meta.total); } })
      .catch((reason) => { if (active && currentRequest === requestId.current) setError(errorMessage(reason)); })
      .finally(() => { if (active && currentRequest === requestId.current) setLoading(false); });
    return () => { active = false; };
  }, [canRead, page]);

  const pages = Math.max(1, Math.ceil(total / pageSize));
  if (!canRead) return <main className="page"><div className="alert" role="alert">You do not have permission to view the audit log.</div></main>;
  return <main className="page audit-page">
    <div className="page-heading"><div><h1>Audit log</h1><p>{total} events</p></div><History size={22} aria-hidden /></div>
    {error && <div className="alert" role="alert">{error}</div>}
    <div className="table-frame"><table className="audit-table"><thead><tr><th>Time</th><th>Operation</th><th>Entity</th><th>Record</th><th>Actor</th></tr></thead><tbody>
      {loading && <tr><td className="empty" colSpan={5}>Loading...</td></tr>}
      {!loading && events.length === 0 && <tr><td className="empty" colSpan={5}>No audit events</td></tr>}
      {!loading && events.map((event) => <AuditRow key={event.id} event={event} />)}
    </tbody></table></div>
    <div className="pagination"><span>Page {page} of {pages}</span><div><button className="icon-button" disabled={page <= 1} onClick={() => setPage((value) => value - 1)} aria-label="Previous page"><ChevronLeft size={17} /></button><button className="icon-button" disabled={page >= pages} onClick={() => setPage((value) => value + 1)} aria-label="Next page"><ChevronRight size={17} /></button></div></div>
  </main>;
}

function AuditRow({ event }: { event: AuditEvent }) {
  return <><tr><td>{new Date(event.occurred_at).toLocaleString()}</td><td><span className={`audit-operation ${event.operation}`}>{event.operation}</span></td><td>{event.entity}</td><td>{event.record_id}</td><td>{event.actor_id ?? "System"}</td></tr><tr className="audit-payload"><td colSpan={5}><details><summary>Change snapshot</summary><div className="audit-json"><section><h2>Before</h2><pre>{formatJson(event.before)}</pre></section><section><h2>After</h2><pre>{formatJson(event.after)}</pre></section></div></details></td></tr></>;
}

function formatJson(value: unknown): string {
  return value === null ? "-" : JSON.stringify(value, null, 2);
}
