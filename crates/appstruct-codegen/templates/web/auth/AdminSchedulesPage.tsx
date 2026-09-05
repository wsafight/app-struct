import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, CirclePlay, Pause, Play } from "lucide-react";
import {
  adminApi,
  adminFeatures,
  type AdminSchedule,
  type AdminScheduleTrigger,
} from "../generated/client";
import { Link, Navigate } from "../navigation";
import { appQueryKeys } from "../query";
import { errorMessage } from "../resource";
import { useAuth } from "./Auth";

type ScheduleOperation = "pause" | "resume" | "trigger";

export function AdminSchedulesPage() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const schedulesQuery = useQuery({
    queryKey: appQueryKeys.admin.schedules,
    queryFn: ({ signal }) => adminApi.listSchedules({ signal }),
    enabled: adminFeatures.jobs && isAdmin,
  });
  const scheduleMutation = useMutation<
    AdminSchedule | AdminScheduleTrigger,
    Error,
    { id: string; operation: ScheduleOperation }
  >({
    mutationFn: ({
      id,
      operation,
    }: {
      id: string;
      operation: ScheduleOperation;
    }) => {
      if (operation === "pause") return adminApi.pauseSchedule(id);
      if (operation === "resume") return adminApi.resumeSchedule(id);
      return adminApi.triggerSchedule(id);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: appQueryKeys.admin.all });
    },
  });
  if (!isAdmin || !adminFeatures.jobs) return <Navigate to="/admin" replace />;
  const schedules: AdminSchedule[] = schedulesQuery.data?.data ?? [];
  const requestError = scheduleMutation.error ?? schedulesQuery.error;
  const error = requestError ? errorMessage(requestError) : "";

  async function mutate(id: string, operation: ScheduleOperation) {
    try {
      await scheduleMutation.mutateAsync({ id, operation });
    } catch {
      // Mutation state renders the request error.
    }
  }

  return (
    <main className="page">
      <Link className="back-link" to="/admin">
        <ArrowLeft size={15} /> Administration
      </Link>
      <div className="page-heading">
        <div>
          <h1>Schedules</h1>
          <p>UTC calendar and interval definitions</p>
        </div>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      <section className="table-frame admin-schedules-table">
        <table>
          <thead>
            <tr>
              <th>Schedule</th>
              <th>Expression</th>
              <th>Queue</th>
              <th>Job kind</th>
              <th>State</th>
              <th>Next run</th>
              <th>Last run</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {schedules.map((schedule) => (
              <tr key={schedule.id}>
                <td title={schedule.id}>
                  <strong>{schedule.name}</strong>
                </td>
                <td>
                  <code>{schedule.cron}</code>
                </td>
                <td>{schedule.queue}</td>
                <td>{schedule.kind}</td>
                <td>
                  <span
                    className={`schedule-status ${scheduleState(schedule)}`}
                  >
                    {scheduleState(schedule)}
                  </span>
                </td>
                <td>{formatDate(schedule.next_run_at)}</td>
                <td>{formatDate(schedule.last_run_at)}</td>
                <td>
                  <div className="row-actions">
                    {schedule.enabled && (
                      <button
                        type="button"
                        className="icon-button"
                        title={
                          schedule.paused ? "Resume schedule" : "Pause schedule"
                        }
                        aria-label={`${schedule.paused ? "Resume" : "Pause"} ${schedule.name}`}
                        disabled={scheduleMutation.isPending}
                        onClick={() =>
                          void mutate(
                            schedule.id,
                            schedule.paused ? "resume" : "pause",
                          )
                        }
                      >
                        {schedule.paused ? (
                          <Play size={15} />
                        ) : (
                          <Pause size={15} />
                        )}
                      </button>
                    )}
                    {schedule.enabled && (
                      <button
                        type="button"
                        className="icon-button"
                        title="Run now"
                        aria-label={`Run ${schedule.name} now`}
                        disabled={scheduleMutation.isPending}
                        onClick={() => void mutate(schedule.id, "trigger")}
                      >
                        <CirclePlay size={15} />
                      </button>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {schedules.length === 0 && !schedulesQuery.isPending && (
          <div className="empty">No schedules configured</div>
        )}
      </section>
    </main>
  );
}

function scheduleState(
  schedule: AdminSchedule,
): "active" | "paused" | "inactive" {
  if (!schedule.enabled) return "inactive";
  return schedule.paused ? "paused" : "active";
}

function formatDate(value: string | null): string {
  return value ? new Date(value).toLocaleString() : "-";
}
