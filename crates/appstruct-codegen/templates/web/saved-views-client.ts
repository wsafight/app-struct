export const savedViewFeatures = {
  server: __SERVER_ENABLED__,
  team: __TEAM_ENABLED__,
} as const;

export type SavedViewVisibility = "private" | "team";

export interface ServerSavedView {
  id: string;
  name: string;
  query: string;
  visibility: SavedViewVisibility;
  revision: number;
  owned: boolean;
  created_at: string;
  updated_at: string;
}

export interface SavedViewInput {
  name: string;
  query: string;
  visibility: SavedViewVisibility;
}

export const savedViewsApi = {
  list: (resource: string, options: RequestOptions = {}) =>
    request<{ data: ServerSavedView[] }>(
      `/api/saved-views?resource=${encodeURIComponent(resource)}`,
      options,
    ),
  create: (resource: string, input: SavedViewInput) =>
    request<ServerSavedView>("/api/saved-views", {
      method: "POST",
      body: JSON.stringify({ resource, ...input }),
    }),
  update: (id: string, input: SavedViewInput, revision: number) =>
    request<ServerSavedView>(`/api/saved-views/${encodeURIComponent(id)}`, {
      method: "PATCH",
      headers: { "If-Match": `"rev-${revision}"` },
      body: JSON.stringify(input),
    }),
  remove: (id: string, revision: number) =>
    request<void>(`/api/saved-views/${encodeURIComponent(id)}`, {
      method: "DELETE",
      headers: { "If-Match": `"rev-${revision}"` },
    }),
};
