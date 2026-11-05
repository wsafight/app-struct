export interface UserFile {
  id: string;
  object_key: string;
  original_name: string;
  content_type: string;
  size: number;
  checksum: string;
  tenant_id: string | null;
  created_at: string;
}

export interface UserFileList {
  data: UserFile[];
  meta: { page: number; page_size: number; total: number };
}
export const fileFeatures = {
  maxBytes: __MAX_BYTES__,
  allowedContentTypes: __ALLOWED_TYPES__,
};

export const fileApi = {
  attachments: (resource: string, recordId: string) => request<{ data: Pick<UserFile, "id" | "original_name" | "content_type" | "size">[] }>(`/api/files/records/${encodeURIComponent(resource)}/${encodeURIComponent(recordId)}`),
  attach: (resource: string, recordId: string, fileId: string) => request<void>(`/api/files/records/${encodeURIComponent(resource)}/${encodeURIComponent(recordId)}/${encodeURIComponent(fileId)}`, { method: "PUT" }),
  detach: (resource: string, recordId: string, fileId: string) => request<void>(`/api/files/records/${encodeURIComponent(resource)}/${encodeURIComponent(recordId)}/${encodeURIComponent(fileId)}`, { method: "DELETE" }),
  replace: (resource: string, recordId: string, fileId: string, replacementId: string) => request<void>(`/api/files/records/${encodeURIComponent(resource)}/${encodeURIComponent(recordId)}/${encodeURIComponent(fileId)}/replace`, { method: "PUT", body: JSON.stringify({ replacement_file_id: replacementId }) }),
  list: (page = 1, pageSize = 25, options: RequestOptions = {}) =>
    request<UserFileList>(
      `/api/files?page=${page}&page_size=${pageSize}`,
      options,
    ),
  remove: (id: string) =>
    request<void>(`/api/files/${encodeURIComponent(id)}`, { method: "DELETE" }),
  upload: async (
    file: File,
    onProgress?: (percentage: number) => void,
    signal?: AbortSignal,
  ): Promise<UserFile> => {
      if (file.size > fileFeatures.maxBytes) {
        throw new Error(`File exceeds the ${fileFeatures.maxBytes} byte limit`);
      }
      if (signal?.aborted) {
        throw new DOMException("Upload cancelled", "AbortError");
      }
      const session = await request<{ id: string; upload_url: string; direct: boolean; expires_at: string }>("/api/files/uploads", { method: "POST", body: JSON.stringify({ name: file.name, content_type: file.type || "application/octet-stream", size: file.size }), signal });
      try {
      await new Promise<void>((resolve, reject) => {
      const upload = new XMLHttpRequest();
      const abort = () => upload.abort();
      signal?.addEventListener("abort", abort, { once: true });
      upload.open("PUT", session.direct ? session.upload_url : `${API_BASE}${session.upload_url}`);
      upload.withCredentials = !session.direct;
      upload.responseType = "text";
      (session.direct ? new Headers({ "Content-Type": file.type || "application/octet-stream" }) : requestHeaders({
        method: "PUT",
        headers: { "Content-Type": file.type || "application/octet-stream" },
      })).forEach((value, key) => upload.setRequestHeader(key, value));
      upload.upload.onprogress = (event) => {
        if (event.lengthComputable)
          onProgress?.(Math.round((event.loaded / event.total) * 95));
      };
      upload.onloadend = () => signal?.removeEventListener("abort", abort);
      upload.onload = () => {
        if (upload.status >= 200 && upload.status < 300) {
          resolve();
        } else {
          reject(new ApiError(upload.status, "UPLOAD_FAILED", "Upload failed; start a new upload or retry."));
        }
      };
      upload.onerror = () =>
        reject(new Error("Upload failed. Check your connection and retry."));
      upload.onabort = () =>
        reject(new DOMException("Upload cancelled", "AbortError"));
      if (signal?.aborted) { signal.removeEventListener("abort", abort); reject(new DOMException("Upload cancelled", "AbortError")); return; }
      upload.send(file);
      });
      const metadata = await request<UserFile>(`/api/files/uploads/${session.id}/complete`, { method: "POST", signal });
      onProgress?.(100);
      return metadata;
      } catch (reason) {
        await request<void>(`/api/files/uploads/${session.id}`, { method: "DELETE" }).catch(() => undefined);
        throw reason;
      }
  },
  download: async (id: string): Promise<Blob> => {
    const path = `/api/files/${encodeURIComponent(id)}/download`;
    const response = await fetch(`${API_BASE}${path}`, {
      credentials: "include",
      headers: requestHeaders(),
    });
    if (!response.ok) {
      const body = (await response
        .json()
        .catch(() => null)) as ErrorEnvelope | null;
      throw new ApiError(
        response.status,
        body?.error?.code ?? "HTTP_ERROR",
        body?.error?.message ?? "Download failed",
      );
    }
    return response.blob();
  },
};
