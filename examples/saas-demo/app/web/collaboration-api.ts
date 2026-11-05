import {
  API_BASE,
  request,
  requestHeaders,
  type Task,
  type RequestOptions,
} from "../../generated/web/src/generated/client";
import { t } from "../../generated/web/src/ui";

export interface Approval {
  id: string;
  task_id: string;
  report_run_id: string;
  requested_by: string;
  reviewer_id: string;
  task_snapshot: Task;
  state: "pending" | "approved" | "rejected" | "cancelled";
  note: string;
  created_at: string;
  completed_at: string | null;
}
export interface Decision {
  id: string;
  actor_id: string;
  decision: string;
  note: string;
  created_at: string;
}
export interface AiWork {
  id: string;
  prompt: string;
  model: string;
  max_tokens: number;
  answer: string;
  state: "queued" | "running" | "succeeded" | "failed" | "cancelled";
  error_code: string | null;
  created_at: string;
}

export const collaborationApi = {
  approvals: (page: number, options: RequestOptions = {}) =>
    request<{ data: Approval[] }>(
      `/api/product/approvals?page=${page}`,
      options,
    ),
  submitApproval: (
    input: {
      task_id: string;
      report_run_id: string;
      reviewer_id: string;
      note: string;
    },
    key: string,
  ) =>
    request<Approval>("/api/product/approvals", {
      method: "POST",
      headers: { "Idempotency-Key": key },
      body: JSON.stringify(input),
    }),
  decide: (id: string, decision: string, note: string) =>
    request<Approval>(`/api/product/approvals/${id}/decide`, {
      method: "POST",
      body: JSON.stringify({ decision, note }),
    }),
  approvalHistory: (id: string, options: RequestOptions = {}) =>
    request<{ data: Decision[] }>(
      `/api/product/approvals/${id}/history`,
      options,
    ),
  approvalReport: async (id: string) => {
    const response = await fetch(
      `${API_BASE}/api/product/approvals/${id}/report`,
      { credentials: "include", headers: requestHeaders() },
    );
    if (!response.ok)
      throw new Error(
        t(
          "The supporting report is unavailable or its retention period expired.",
        ),
      );
    return response.blob();
  },
  aiCapabilities: (options: RequestOptions = {}) =>
    request<{
      enabled: boolean;
      provider: string;
      credit_cost: number;
      max_prompt_bytes: number;
      max_output_bytes: number;
      max_parallel_per_team: number;
    }>("/api/product/assistant/capabilities", options),
  aiHistory: (page: number, options: RequestOptions = {}) =>
    request<{ data: AiWork[] }>(`/api/product/assistant?page=${page}`, options),
  ask: (prompt: string, key: string) =>
    request<AiWork>("/api/product/assistant", {
      method: "POST",
      headers: { "Idempotency-Key": key },
      body: JSON.stringify({ prompt }),
    }),
  cancelAi: (id: string) =>
    request<AiWork>(`/api/product/assistant/${id}/cancel`, { method: "POST" }),
};

export async function followAi(
  id: string,
  signal: AbortSignal,
  receive: (work: AiWork) => void,
) {
  const response = await fetch(
    `${API_BASE}/api/product/assistant/${encodeURIComponent(id)}/events`,
    { credentials: "include", headers: requestHeaders(), signal },
  );
  if (!response.ok || !response.body)
    throw new Error(t("Live progress is unavailable. Refresh your history."));
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  try {
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      buffer += decoder.decode(chunk.value, { stream: true });
      if (buffer.length > 524288)
        throw new Error(t("Live progress exceeded its size limit."));
      let boundary = buffer.indexOf("\n\n");
      while (boundary >= 0) {
        const event = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);
        if (event.startsWith("event: snapshot\n")) {
          const data = event
            .split("\n")
            .filter((line) => line.startsWith("data: "))
            .map((line) => line.slice(6))
            .join("\n");
          receive(JSON.parse(data) as AiWork);
        }
        if (event.startsWith("event: error\n"))
          throw new Error(
            t("Live progress interrupted. Refresh your history."),
          );
        boundary = buffer.indexOf("\n\n");
      }
    }
  } finally {
    await reader.cancel().catch(() => undefined);
    reader.releaseLock();
  }
}
