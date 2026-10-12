pub(super) fn runtime_source() -> &'static str {
    r#"async function requestText(path: string, init?: RequestInit): Promise<string> {
  const headers = new Headers(init?.headers);
  const method = init?.method ?? "GET";
  if (method !== "GET" && method !== "HEAD") {
    const csrf = cookieValue("appstruct_csrf");
    if (csrf) headers.set("X-CSRF-Token", csrf);
  }
  const tenant = currentTenant();
  if (tenant) headers.set("X-AppStruct-Tenant", tenant);
  const response = await fetch(`${API_BASE}${path}`, { ...init, headers, credentials: "include" });
  if (!response.ok) throw new ApiError(response.status, "HTTP_ERROR", `Request failed with status ${response.status}`);
  return response.text();
}

interface CsvWritable { write(chunk: Uint8Array): Promise<void>; close(): Promise<void>; abort(): Promise<void>; }
type CsvPicker = (options: { suggestedName: string; types: { description: string; accept: Record<string, string[]> }[] }) => Promise<{ createWritable(): Promise<CsvWritable> }>;

async function downloadResourceCsv(path: string, filename: string): Promise<void> {
  const picker = (window as unknown as { showSaveFilePicker?: CsvPicker }).showSaveFilePicker;
  let output: CsvWritable | undefined;
  if (picker) {
    try {
      const file = await picker.call(window, { suggestedName: filename, types: [{ description: "CSV", accept: { "text/csv": [".csv"] } }] });
      output = await file.createWritable();
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return;
      throw error;
    }
  }
  const controller = new AbortController();
  try {
    const response = await fetch(`${API_BASE}${path}`, { headers: requestHeaders(), credentials: "include", signal: controller.signal });
    if (!response.ok) throw new ApiError(response.status, "HTTP_ERROR", `Request failed with status ${response.status}`);
    if (output) {
      if (response.body) {
        const reader = response.body.getReader();
        try {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            await output.write(value);
          }
        } finally { reader.releaseLock(); }
      } else {
        await output.write(new Uint8Array(await response.arrayBuffer()));
      }
      await output.close();
    } else {
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url; link.download = filename; link.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    }
  } catch (error) {
    controller.abort();
    await output?.abort();
    throw error;
  }
}

export interface BulkFailure { id: string; code: string; message: string; }
export interface BulkResult { succeeded: string[]; failed: BulkFailure[]; }
export interface BulkUpdateRequest<T> { ids: string[]; patch: T; expected_revisions: Record<string, number>; }
export interface BulkDeleteRequest { ids: string[]; expected_revisions: Record<string, number>; }
"#
}
