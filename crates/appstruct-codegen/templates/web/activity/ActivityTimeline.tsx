import { formatDateTime } from "../ui";
import { t } from "../ui";
import {
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import {
  Download,
  LoaderCircle,
  MessageSquare,
  Paperclip,
  Send,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import { type FormEvent, useRef, useState } from "react";
import {
  activityApi,
  type ActivityAttachmentInput,
  type ActivityEntry,
  type ActivityResource,
} from "../generated/client";
import { appQueryKeys } from "../query";
import type { ResourceDefinition } from "../resource";
import { errorMessage, triggerDownload, useResourceActor } from "../resource";
import { useActivityRealtime } from "./useActivityRealtime";

const PAGE_SIZE = 20;

export function ActivityTimeline({
  resource,
  recordId,
}: {
  resource: ResourceDefinition;
  recordId: string;
}) {
  const config = resource.activity;
  const actor = useResourceActor();
  const queryClient = useQueryClient();
  const [comment, setComment] = useState("");
  const [attachment, setAttachment] = useState<File | null>(null);
  const [moderating, setModerating] = useState<string | null>(null);
  const [reason, setReason] = useState("");
  const fileInput = useRef<HTMLInputElement>(null);
  const activityResource = resource.slug as ActivityResource;
  const queryKey = appQueryKeys.activity(resource.slug, recordId);
  useActivityRealtime(Boolean(config), resource, recordId);
  const query = useInfiniteQuery({
    queryKey,
    queryFn: ({ pageParam, signal }) =>
      activityApi.list(
        activityResource,
        recordId,
        pageParam || undefined,
        PAGE_SIZE,
        { signal },
      ),
    initialPageParam: "",
    getNextPageParam: (page) => page.meta.next_cursor ?? undefined,
  });
  const refresh = () => queryClient.invalidateQueries({ queryKey });
  const create = useMutation({
    mutationFn: async () => {
      const input: { body: string; attachment?: ActivityAttachmentInput } = {
        body: comment,
      };
      if (attachment) input.attachment = await fileInputValue(attachment);
      return activityApi.comment(activityResource, recordId, input);
    },
    onSuccess: async () => {
      setComment("");
      setAttachment(null);
      if (fileInput.current) fileInput.current.value = "";
      await refresh();
    },
  });
  const withdraw = useMutation({
    mutationFn: (entryId: string) =>
      activityApi.withdraw(activityResource, recordId, entryId),
    onSuccess: refresh,
  });
  const moderate = useMutation({
    mutationFn: ({ entryId, note }: { entryId: string; note: string }) =>
      activityApi.moderate(activityResource, recordId, entryId, note),
    onSuccess: async () => {
      setModerating(null);
      setReason("");
      await refresh();
    },
  });

  if (!config) return null;
  const entries = query.data?.pages.flatMap((page) => page.data) ?? [];
  const commentBytes = new TextEncoder().encode(comment.trim()).length;
  const validComment =
    commentBytes > 0 && commentBytes <= config.maxCommentBytes;
  const isAdmin =
    actor?.roles.some((role) => config.adminRoles.includes(role)) ?? false;
  const mutationError = create.error ?? withdraw.error ?? moderate.error;

  function submit(event: FormEvent) {
    event.preventDefault();
    if (validComment && !create.isPending) create.mutate();
  }

  async function download(entry: ActivityEntry) {
    try {
      const blob = await activityApi.download(
        activityResource,
        recordId,
        entry.id,
      );
      triggerDownload(blob, entry.attachment_name ?? "attachment");
    } catch (error) {
      window.alert(errorMessage(error));
    }
  }

  return (
    <section className="activity-timeline" aria-label={t("Record activity")}>
      <div className="activity-heading">
        <MessageSquare size={17} />
        <h2>{t("Activity")}</h2>
      </div>
      <form className="activity-composer" onSubmit={submit}>
        <textarea
          value={comment}
          onChange={(event) => setComment(event.target.value)}
          placeholder={t("Add a comment")}
          rows={3}
        />
        <div className="activity-composer-actions">
          <span
            className={
              commentBytes > config.maxCommentBytes ? "limit-error" : ""
            }
          >
            {commentBytes}/{config.maxCommentBytes} {t("bytes")}
          </span>
          {config.attachments && (
            <label className="attachment-picker">
              <Paperclip size={15} />
              <span>{attachment?.name ?? "Attach file"}</span>
              <input
                ref={fileInput}
                type="file"
                onChange={(event) =>
                  setAttachment(event.target.files?.item(0) ?? null)
                }
              />
            </label>
          )}
          <button
            type="submit"
            className="primary-button"
            disabled={!validComment || create.isPending}
          >
            {create.isPending ? (
              <LoaderCircle className="spinner" size={15} />
            ) : (
              <Send size={15} />
            )}
            {t("Comment")}{" "}
          </button>
        </div>
      </form>
      {(query.error || mutationError) && (
        <div className="alert" role="alert">
          {errorMessage(query.error ?? mutationError)}
        </div>
      )}
      <div className="activity-list">
        {query.isPending && (
          <div className="empty">{t("Loading activity...")}</div>
        )}
        {!query.isPending && entries.length === 0 && (
          <div className="empty"> {t("No activity yet")}</div>
        )}
        {entries.map((entry) => (
          <article
            className={`activity-entry ${entry.kind} ${entry.withdrawn_at ? "withdrawn" : ""}`}
            key={entry.id}
          >
            <div className="activity-entry-marker" />
            <div className="activity-entry-content">
              <header>
                <strong>{entry.actor_id ?? "System"}</strong>
                <time dateTime={entry.occurred_at}>
                  {formatDateTime(entry.occurred_at)}
                </time>
              </header>
              {entry.withdrawn_at ? (
                <p className="activity-tombstone">
                  {entry.governance_reason
                    ? t("Removed by a moderator: {0}", {
                        "0": entry.governance_reason,
                      })
                    : t("Comment withdrawn")}
                </p>
              ) : entry.kind === "system" ? (
                <p className="activity-system-event">
                  {formatEvent(entry.event)}
                </p>
              ) : (
                <p>{entry.body}</p>
              )}
              {!entry.withdrawn_at && entry.attachment_file_id && (
                <button
                  type="button"
                  className="attachment-download"
                  onClick={() => void download(entry)}
                >
                  <Download size={15} />
                  {entry.attachment_name ?? "Attachment"}
                </button>
              )}
              {!entry.withdrawn_at && entry.kind === "comment" && (
                <div className="activity-entry-actions">
                  {entry.actor_id === actor?.id && (
                    <button
                      type="button"
                      className="icon-button danger"
                      title={t("Withdraw comment")}
                      aria-label={t("Withdraw comment")}
                      disabled={withdraw.isPending}
                      onClick={() => withdraw.mutate(entry.id)}
                    >
                      <Trash2 size={15} />
                    </button>
                  )}
                  {isAdmin && (
                    <button
                      type="button"
                      className="icon-button"
                      title={t("Moderate comment")}
                      aria-label={t("Moderate comment")}
                      onClick={() => {
                        setModerating(entry.id);
                        setReason("");
                      }}
                    >
                      <ShieldAlert size={15} />
                    </button>
                  )}
                </div>
              )}
              {moderating === entry.id && (
                <form
                  className="moderation-form"
                  onSubmit={(event) => {
                    event.preventDefault();
                    if (reason.trim())
                      moderate.mutate({ entryId: entry.id, note: reason });
                  }}
                >
                  <input
                    value={reason}
                    onChange={(event) => setReason(event.target.value)}
                    placeholder={t("Moderation reason")}
                    maxLength={1000}
                    autoFocus
                  />
                  <button
                    type="submit"
                    className="danger-button"
                    disabled={!reason.trim() || moderate.isPending}
                  >
                    {t("Remove")}
                  </button>
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => setModerating(null)}
                  >
                    {t("Cancel")}
                  </button>
                </form>
              )}
            </div>
          </article>
        ))}
      </div>
      {query.hasNextPage && (
        <button
          type="button"
          className="secondary-button activity-load-more"
          disabled={query.isFetchingNextPage}
          onClick={() => void query.fetchNextPage()}
        >
          {query.isFetchingNextPage ? "Loading..." : t("Load earlier activity")}
        </button>
      )}
    </section>
  );
}

function formatEvent(event: string | null): string {
  if (!event) return t("Record updated");
  return event
    .split(".")
    .map((part) => part.replaceAll("_", " "))
    .join(" · ");
}

function fileInputValue(file: File): Promise<ActivityAttachmentInput> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () =>
      reject(new Error(t("The attachment could not be read")));
    reader.onload = () => {
      const result = String(reader.result ?? "");
      const separator = result.indexOf(",");
      if (separator < 0) {
        reject(new Error(t("The attachment could not be encoded")));
        return;
      }
      resolve({
        name: file.name,
        content_type: file.type || "application/octet-stream",
        content_base64: result.slice(separator + 1),
      });
    };
    reader.readAsDataURL(file);
  });
}
