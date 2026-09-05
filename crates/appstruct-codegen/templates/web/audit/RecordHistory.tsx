import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, History } from "lucide-react";
import { useEffect, useState } from "react";
import { auditApi, type AuditEvent } from "../generated/client";
import { auditAccess } from "../generated/resources";
import { appQueryKeys } from "../query";
import { errorMessage, useCanAccessRule } from "../resource";
import { diffSnapshots } from "./AuditPage";

const PAGE_SIZE = 5;

export function RecordHistory({
  entity,
  recordId,
}: {
  entity: string;
  recordId: string;
}) {
  const canRead = useCanAccessRule(auditAccess);
  const [page, setPage] = useState(1);
  useEffect(() => setPage(1), [entity, recordId]);
  const query = useQuery({
    queryKey: appQueryKeys.audit(page, PAGE_SIZE, entity, recordId),
    queryFn: ({ signal }) =>
      auditApi.list(
        { page, page_size: PAGE_SIZE, entity, record_id: recordId },
        { signal },
      ),
    enabled: canRead,
    placeholderData: (previous) => previous,
  });
  if (!canRead) return null;
  const events = query.data?.data ?? [];
  const total = query.data?.meta.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  return (
    <section className="record-history" aria-label="Record history">
      <div className="record-history-heading">
        <History size={17} />
        <h2>History</h2>
      </div>
      {query.error && (
        <div className="alert" role="alert">
          {errorMessage(query.error)}
        </div>
      )}
      <div className="history-list">
        {query.isPending && <div className="empty">Loading history...</div>}
        {!query.isPending && events.length === 0 && (
          <div className="empty">No changes recorded</div>
        )}
        {events.map((event) => (
          <HistoryEvent key={event.id} event={event} />
        ))}
      </div>
      {pages > 1 && (
        <div className="pagination">
          <span>
            Page {page} of {pages}
          </span>
          <div>
            <button
              type="button"
              className="icon-button"
              disabled={page <= 1}
              onClick={() => setPage((value) => value - 1)}
              aria-label="Previous history page"
            >
              <ChevronLeft size={17} />
            </button>
            <button
              type="button"
              className="icon-button"
              disabled={page >= pages}
              onClick={() => setPage((value) => value + 1)}
              aria-label="Next history page"
            >
              <ChevronRight size={17} />
            </button>
          </div>
        </div>
      )}
    </section>
  );
}

function HistoryEvent({ event }: { event: AuditEvent }) {
  const changes = diffSnapshots(event.before, event.after);
  return (
    <article className="history-event">
      <div className="history-event-header">
        <span className={`audit-operation ${event.operation}`}>
          {event.operation}
        </span>
        <time dateTime={event.occurred_at}>
          {new Date(event.occurred_at).toLocaleString()}
        </time>
        <span>{event.actor_id ?? "System"}</span>
      </div>
      <details>
        <summary>{changes.length} changed fields</summary>
        <dl className="history-changes">
          {changes.flatMap((change) => [
            <dt key={`${change.field}-name`}>{change.field}</dt>,
            <dd key={`${change.field}-before`}>
              {formatValue(change.before)}
            </dd>,
            <dd key={`${change.field}-after`}>{formatValue(change.after)}</dd>,
          ])}
        </dl>
      </details>
    </article>
  );
}

function formatValue(value: unknown): string {
  if (value === undefined || value === null) return "-";
  return typeof value === "object" ? JSON.stringify(value) : String(value);
}
