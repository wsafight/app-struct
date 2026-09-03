import { Bookmark, Copy, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useSearchParams } from "../../navigation";

interface SavedView {
  id: string;
  name: string;
  query: string;
}

export function SavedViews({
  resourceId,
  actorId,
  onError,
}: {
  resourceId: string;
  actorId?: string;
  onError: (message: string) => void;
}) {
  const [searchParams, setSearchParams] = useSearchParams();
  const [savedViews, setSavedViews] = useState<SavedView[]>([]);
  const [viewName, setViewName] = useState("");
  const storageKey = `appstruct.saved-views.${resourceId}.${actorId ?? "anonymous"}.${tenantStorageScope()}`;

  useEffect(() => {
    try {
      setSavedViews(
        JSON.parse(localStorage.getItem(storageKey) ?? "[]") as SavedView[],
      );
    } catch {
      setSavedViews([]);
    }
  }, [storageKey]);

  function persist(next: SavedView[]) {
    setSavedViews(next);
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
      onError("");
    } catch {
      onError("Could not save this view in the browser");
    }
  }

  function saveView() {
    const name = viewName.trim();
    if (!name) return;
    const view = {
      id: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
      name,
      query: searchParams.toString(),
    };
    persist([...savedViews.filter((item) => item.name !== name), view]);
    setViewName("");
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
        value=""
        onChange={(event) => {
          const view = savedViews.find(
            (item) => item.id === event.target.value,
          );
          if (view) setSearchParams(new URLSearchParams(view.query));
        }}
      >
        <option value="">Saved views</option>
        {savedViews.map((view) => (
          <option key={view.id} value={view.id}>
            {view.name}
          </option>
        ))}
      </select>
      <input
        aria-label="View name"
        placeholder="Name this view"
        value={viewName}
        onChange={(event) => setViewName(event.target.value)}
      />
      <button
        className="secondary-button"
        onClick={saveView}
        disabled={!viewName.trim()}
      >
        Save
      </button>
      <button
        className="icon-button"
        onClick={() => void copyViewLink()}
        title="Copy view link"
        aria-label="Copy view link"
      >
        <Copy size={16} />
      </button>
      {savedViews.length > 0 && (
        <button
          className="icon-button danger"
          onClick={() => persist(savedViews.slice(0, -1))}
          title="Delete last saved view"
          aria-label="Delete last saved view"
        >
          <Trash2 size={16} />
        </button>
      )}
    </div>
  );
}

function tenantStorageScope(): string {
  try {
    return window.localStorage.getItem("appstruct_tenant") ?? "global";
  } catch {
    return "global";
  }
}
