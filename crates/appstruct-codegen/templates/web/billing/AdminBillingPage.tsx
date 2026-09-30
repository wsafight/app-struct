import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { ArrowLeft, RotateCcw } from "lucide-react";
import { Link, Navigate } from "../navigation";
import { billingAdminApi, type BillingEvent } from "../generated/client";
import { appQueryKeys } from "../query";
import { errorMessage } from "../resource";
import { AdminPagination } from "../auth/AuthPages";
import { useAuth } from "../auth/Auth";

export function AdminBillingPage() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const [page, setPage] = useState(1);
  const pageSize = 25;
  const queryKey = appQueryKeys.admin.billingEvents(page, pageSize);
  const eventsQuery = useQuery({
    queryKey,
    queryFn: ({ signal }) => billingAdminApi.events(page, pageSize, { signal }),
    enabled: isAdmin,
    placeholderData: (previous) => previous,
  });
  const replayMutation = useMutation({
    mutationFn: billingAdminApi.replayEvent,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: appQueryKeys.admin.all });
    },
  });
  if (!isAdmin) return <Navigate to="/admin" replace />;
  const events: BillingEvent[] = eventsQuery.data?.data ?? [];
  const total = eventsQuery.data?.meta.total ?? 0;
  const requestError = replayMutation.error ?? eventsQuery.error;
  const error = requestError ? errorMessage(requestError) : "";

  return (
    <main className="page">
      <Link className="back-link" to="/admin">
        <ArrowLeft size={15} /> Administration
      </Link>
      <div className="page-heading">
        <div>
          <h1>Billing events</h1>
          <p>Inspect Stripe subscription events and replay reconciliation.</p>
        </div>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      <section className="table-frame admin-billing-table">
        <table>
          <thead>
            <tr>
              <th>Event</th>
              <th>Type</th>
              <th>Received</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {events.map((event) => (
              <tr key={event.event_id}>
                <td title={event.event_id}>
                  <code>{event.event_id}</code>
                </td>
                <td>{event.event_type}</td>
                <td>{new Date(event.created_at).toLocaleString()}</td>
                <td>
                  <button
                    type="button"
                    className="icon-button"
                    title="Replay subscription reconciliation"
                    aria-label={`Replay ${event.event_id}`}
                    disabled={replayMutation.isPending}
                    onClick={() => replayMutation.mutate(event.event_id)}
                  >
                    <RotateCcw size={15} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {events.length === 0 && !error && (
          <div className="empty">No Billing events yet</div>
        )}
      </section>
      <AdminPagination
        page={page}
        pageSize={pageSize}
        total={total}
        onPageChange={setPage}
      />
    </main>
  );
}
