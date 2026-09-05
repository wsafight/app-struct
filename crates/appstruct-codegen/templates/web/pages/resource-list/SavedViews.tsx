import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Bookmark, Copy, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  savedViewFeatures,
  savedViewsApi,
  type SavedViewVisibility,
  type ServerSavedView,
} from "../../generated/client";
import { useSearchParams } from "../../navigation";

interface LocalSavedView {
  id: string;
  name: string;
  query: string;
}

interface DisplayView {
  id: string;
  name: string;
  query: string;
  source: "server" | "browser";
  visibility: SavedViewVisibility | "browser";
  revision: number;
  owned: boolean;
}

type SaveTarget = SavedViewVisibility | "browser";

export function SavedViews({
  resourceId,
  actorId,
  onError,
}: {
  resourceId: string;
  actorId?: string;
  onError: (message: string) => void;
}) {
  const queryClient = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const [localViews, setLocalViews] = useState<LocalSavedView[]>([]);
  const [viewName, setViewName] = useState("");
  const [saveTarget, setSaveTarget] = useState<SaveTarget>(
    savedViewFeatures.server ? "private" : "browser",
  );
  const [selectedViewKey, setSelectedViewKey] = useState("");
  const [busy, setBusy] = useState(false);
  const tenantScope = tenantStorageScope();
  const storageKey = `appstruct.saved-views.${resourceId}.${actorId ?? "anonymous"}.${tenantScope}`;
  const serverKey = [
    "saved-views",
    resourceId,
    actorId ?? "anonymous",
    tenantScope,
  ] as const;
  const serverQuery = useQuery({
    queryKey: serverKey,
    queryFn: ({ signal }) => savedViewsApi.list(resourceId, { signal }),
    enabled: savedViewFeatures.server && Boolean(actorId),
  });
  const views = useMemo<DisplayView[]>(
    () => [
      ...(serverQuery.data?.data ?? []).map(serverDisplayView),
      ...localViews.map((view) => ({
        ...view,
        source: "browser" as const,
        visibility: "browser" as const,
        revision: 0,
        owned: true,
      })),
    ],
    [localViews, serverQuery.data?.data],
  );
  const activeView =
    views.find(
      (view) =>
        viewKey(view) === selectedViewKey &&
        view.query === searchParams.toString(),
    ) ?? views.find((view) => view.query === searchParams.toString());

  useEffect(() => {
    setSelectedViewKey("");
    try {
      setLocalViews(parseLocalSavedViews(localStorage.getItem(storageKey)));
    } catch {
      setLocalViews([]);
    }
  }, [storageKey]);

  useEffect(() => {
    if (serverQuery.error)
      onError("Could not load saved views from the server");
  }, [onError, serverQuery.error]);

  function persistLocal(next: LocalSavedView[]) {
    setLocalViews(next);
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
      onError("");
    } catch {
      onError("Could not save this view in the browser");
    }
  }

  async function saveView() {
    const name = viewName.trim();
    if (!name || busy) return;
    if (saveTarget === "browser") {
      const existing = localViews.find((item) => item.name === name);
      const view = {
        id:
          existing?.id ??
          `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
        name,
        query: searchParams.toString(),
      };
      persistLocal([...localViews.filter((item) => item.name !== name), view]);
      setSelectedViewKey(`browser:${view.id}`);
      setViewName("");
      return;
    }
    setBusy(true);
    try {
      const input = {
        name,
        query: searchParams.toString(),
        visibility: saveTarget,
      };
      const existing = serverQuery.data?.data.find(
        (view) => view.owned && view.name === name,
      );
      const saved = existing
        ? await savedViewsApi.update(existing.id, input, existing.revision)
        : await savedViewsApi.create(resourceId, input);
      await queryClient.invalidateQueries({ queryKey: serverKey });
      setSelectedViewKey(`server:${saved.id}`);
      setViewName("");
      onError("");
    } catch (error) {
      onError(
        error instanceof Error ? error.message : "Could not save this view",
      );
    } finally {
      setBusy(false);
    }
  }

  async function deleteActiveView() {
    if (!activeView || busy) return;
    if (activeView.source === "browser") {
      persistLocal(localViews.filter((view) => view.id !== activeView.id));
      setSelectedViewKey("");
      return;
    }
    setBusy(true);
    try {
      await savedViewsApi.remove(activeView.id, activeView.revision);
      await queryClient.invalidateQueries({ queryKey: serverKey });
      setSelectedViewKey("");
      onError("");
    } catch (error) {
      onError(
        error instanceof Error ? error.message : "Could not delete this view",
      );
    } finally {
      setBusy(false);
    }
  }

  async function copyViewLink() {
    try {
      await navigator.clipboard.writeText(window.location.href);
      onError("");
    } catch {
      onError("Could not copy view link");
    }
  }

  return (
    <div className="view-toolbar">
      <Bookmark size={16} />
      <select
        aria-label="Saved views"
        value={activeView ? viewKey(activeView) : ""}
        onChange={(event) => {
          setSelectedViewKey(event.target.value);
          const view = views.find(
            (item) => viewKey(item) === event.target.value,
          );
          if (view) setSearchParams(new URLSearchParams(view.query));
        }}
      >
        <option value="">Saved views</option>
        {views.map((view) => (
          <option key={viewKey(view)} value={viewKey(view)}>
            {view.name} ({viewLabel(view)})
          </option>
        ))}
      </select>
      <input
        aria-label="View name"
        placeholder="Name this view"
        maxLength={80}
        value={viewName}
        onChange={(event) => setViewName(event.target.value)}
      />
      <select
        aria-label="Save view as"
        value={saveTarget}
        onChange={(event) => setSaveTarget(event.target.value as SaveTarget)}
      >
        {savedViewFeatures.server && <option value="private">Private</option>}
        {savedViewFeatures.team && <option value="team">Team</option>}
        <option value="browser">Browser only</option>
      </select>
      <button
        type="button"
        className="secondary-button"
        onClick={() => void saveView()}
        disabled={!viewName.trim() || busy}
      >
        Save
      </button>
      <button
        type="button"
        className="icon-button"
        onClick={() => void copyViewLink()}
        title="Copy view link"
        aria-label="Copy view link"
      >
        <Copy size={16} />
      </button>
      {activeView?.owned && (
        <button
          type="button"
          className="icon-button danger"
          onClick={() => void deleteActiveView()}
          disabled={busy}
          title="Delete selected view"
          aria-label="Delete selected view"
        >
          <Trash2 size={16} />
        </button>
      )}
    </div>
  );
}

function serverDisplayView(view: ServerSavedView): DisplayView {
  return { ...view, source: "server" };
}

function viewKey(view: DisplayView): string {
  return `${view.source}:${view.id}`;
}

function viewLabel(view: DisplayView): string {
  if (view.visibility === "browser") return "Browser";
  if (view.visibility === "private") return "Private";
  return view.owned ? "Team" : "Team shared";
}

export function parseLocalSavedViews(value: string | null): LocalSavedView[] {
  if (!value) return [];
  try {
    const parsed: unknown = JSON.parse(value);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (view): view is LocalSavedView =>
        view !== null &&
        typeof view === "object" &&
        typeof (view as LocalSavedView).id === "string" &&
        typeof (view as LocalSavedView).name === "string" &&
        typeof (view as LocalSavedView).query === "string",
    );
  } catch {
    return [];
  }
}

function tenantStorageScope(): string {
  try {
    return window.localStorage.getItem("appstruct_tenant") ?? "global";
  } catch {
    return "global";
  }
}
