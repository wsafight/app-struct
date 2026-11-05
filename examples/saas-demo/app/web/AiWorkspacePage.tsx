import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  currentTenant,
  productApi,
} from "../../generated/web/src/generated/client";
import { errorMessage } from "../../generated/web/src/resource";
import { Link } from "../../generated/web/src/navigation";
import { collaborationApi, followAi, type AiWork } from "./collaboration-api";
import {
  appQueryKeys,
  workRefreshInterval,
} from "../../generated/web/src/query";
import {
  ActionFeedback,
  EmptyState,
  LoadingState,
  formatDateTime,
  formatNumber,
  statusLabel,
  t,
} from "../../generated/web/src/ui";

export function AiWorkspacePage() {
  const client = useQueryClient();
  const tenant = currentTenant();
  const [prompt, setPrompt] = useState("");
  const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<string | null>(null);
  const [live, setLive] = useState<AiWork | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const submission = useRef<{
    prompt: string;
    tenant: string | undefined;
    key: string;
  } | null>(null);
  const capabilities = useQuery({
    queryKey: ["ai-capabilities", tenant],
    queryFn: ({ signal }) => collaborationApi.aiCapabilities({ signal }),
  });
  const history = useQuery({
    queryKey: ["ai-history", tenant, page],
    queryFn: ({ signal }) => collaborationApi.aiHistory(page, { signal }),
    refetchInterval: (query) => workRefreshInterval(query.state.data?.data),
    refetchOnWindowFocus: true,
  });
  const credits = useQuery({
    queryKey: appQueryKeys.product.credits(),
    queryFn: ({ signal }) => productApi.credits(1, { signal }),
  });
  const promptBytes = new TextEncoder().encode(prompt).length;
  const maxPromptBytes = capabilities.data?.max_prompt_bytes ?? 8000;
  const current =
    live?.id === selected
      ? live
      : history.data?.data.find((item) => item.id === selected);
  const active = current && ["queued", "running"].includes(current.state);
  useEffect(() => {
    if (!selected || !active) return;
    const controller = new AbortController();
    void followAi(selected, controller.signal, (work) => {
      setLive(work);
      if (!["queued", "running"].includes(work.state)) {
        void client.invalidateQueries({ queryKey: ["ai-history"] });
        void client.invalidateQueries({ queryKey: appQueryKeys.product.all });
      }
    }).catch((reason: unknown) => {
      if (!controller.signal.aborted) setError(errorMessage(reason));
    });
    return () => controller.abort();
  }, [selected, active, tenant, client]);
  async function submit() {
    if (
      pending ||
      promptBytes > maxPromptBytes ||
      !prompt.trim() ||
      !capabilities.data?.enabled
    )
      return;
    setPending(true);
    setError("");
    setNotice("");
    try {
      if (
        submission.current?.prompt !== prompt ||
        submission.current.tenant !== tenant
      )
        submission.current = { prompt, tenant, key: crypto.randomUUID() };
      const work = await collaborationApi.ask(prompt, submission.current.key);
      submission.current = null;
      setLive(work);
      setSelected(work.id);
      setPrompt("");
      setNotice(t("Draft requested. Follow its progress below."));
      await client.invalidateQueries({ queryKey: ["ai-history"] });
      await client.invalidateQueries({ queryKey: ["product", "credits"] });
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }
  async function cancel() {
    if (!selected) return;
    setPending(true);
    setError("");
    try {
      setLive(await collaborationApi.cancelAi(selected));
      setNotice(t("Request cancelled. Reserved credits were returned."));
      await client.invalidateQueries({ queryKey: ["ai-history"] });
      await client.invalidateQueries({ queryKey: ["product", "credits"] });
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }
  return (
    <main className="page">
      <h1>{t("AI writing assistant")}</h1>
      <p>
        {t(
          "Optional Ollama adapter. Each request reserves one credit; final failure or cancellation returns it. Only send data approved for your configured model service.",
        )}
      </p>
      <p>
        {t("Available credits")}:{" "}
        {credits.data ? formatNumber(credits.data.available) : "—"} ·{" "}
        <Link to="/credits">{t("Manage credits")}</Link>
      </p>
      {capabilities.data && !capabilities.data.enabled && (
        <p role="status">
          {t(
            "AI is disabled. Configure APPSTRUCT_AI_ENABLED, APPSTRUCT_AI_MODEL and the server-side Ollama endpoint. Other product features work without a model service.",
          )}
        </p>
      )}
      <ActionFeedback
        error={
          error ||
          (history.error || capabilities.error || credits.error
            ? errorMessage(history.error || capabilities.error || credits.error)
            : "")
        }
        notice={notice}
        onRetry={() => {
          void history.refetch();
          void capabilities.refetch();
          void credits.refetch();
        }}
      />
      <form
        className="panel"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <label>
          {t("What should the assistant draft?")}
          <textarea
            required
            maxLength={maxPromptBytes}
            aria-describedby="prompt-size"
            aria-invalid={promptBytes > maxPromptBytes}
            value={prompt}
            disabled={pending}
            onChange={(event) => setPrompt(event.target.value)}
          />
        </label>
        <p
          id="prompt-size"
          role={promptBytes > maxPromptBytes ? "alert" : undefined}
        >
          {t("{used} / {limit} UTF-8 bytes", {
            used: formatNumber(promptBytes),
            limit: formatNumber(maxPromptBytes),
          })}
        </p>
        <p>
          {t(
            "Up to 8000 UTF-8 bytes per prompt, 2048 output tokens maximum, 64 KiB output, and two pending requests per team.",
          )}
        </p>
        <button
          disabled={
            pending ||
            !capabilities.data?.enabled ||
            !prompt.trim() ||
            promptBytes > maxPromptBytes
          }
        >
          {t(pending ? "Submitting..." : "Generate draft")}
        </button>
      </form>
      {current && (
        <section className="panel" aria-live="polite">
          <h2>
            {statusLabel(current.state)} · {current.model}
          </h2>
          <p>{current.prompt}</p>
          <pre className="ai-answer">{current.answer}</pre>
          {current.error_code && (
            <p role="alert">
              {t(
                "Generation failed. Reserved credits were returned; you can try a new request.",
              )}{" "}
              <code>{current.error_code}</code>
            </p>
          )}
          {active && (
            <button
              type="button"
              disabled={pending}
              onClick={() => void cancel()}
            >
              {t("Cancel and refund")}
            </button>
          )}
        </section>
      )}
      <h2>{t("Your history")}</h2>
      {history.isPending && <LoadingState />}
      {history.data?.data.length === 0 && (
        <EmptyState
          title={t("No drafts yet")}
          description={t("Submit a prompt to create your first draft.")}
        />
      )}
      {history.data?.data.map((item) => (
        <button
          type="button"
          key={item.id}
          className="ai-history-item"
          aria-pressed={selected === item.id}
          onClick={() => {
            setSelected(item.id);
            setLive(item);
          }}
        >
          {formatDateTime(item.created_at)} · {statusLabel(item.state)} ·{" "}
          {item.prompt.slice(0, 60)}
        </button>
      ))}
      <button
        type="button"
        disabled={page === 1 || history.isFetching}
        onClick={() => setPage(page - 1)}
      >
        {t("Previous")}
      </button>
      <button
        type="button"
        disabled={
          !history.data || history.data.data.length < 25 || history.isFetching
        }
        onClick={() => setPage(page + 1)}
      >
        {t("Next")}
      </button>
    </main>
  );
}
