import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { fileApi } from "../generated/client";
import { errorMessage, triggerDownload } from "../resource";
import { t } from "../ui";
import { FileUpload } from "./FilesPage";

export function RecordAttachments({ resource, recordId, writable }: { resource: string; recordId: string; writable: boolean }) {
  const client = useQueryClient();
  const key = ["attachments", resource, recordId];
  const files = useQuery({ queryKey: key, queryFn: () => fileApi.attachments(resource, recordId) });
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const [replacing, setReplacing] = useState<string | null>(null);
  async function action(operation: () => Promise<unknown>) {
    setPending(true);
    setError("");
    try { await operation(); await client.invalidateQueries({ queryKey: key }); }
    catch (reason) { setError(errorMessage(reason)); }
    finally { setPending(false); }
  }
  return <section className="panel" aria-label={t("Attachments")}>
    <h2>{t("Attachments")}</h2>
    {files.isPending && <p role="status">{t("Loading")}</p>}
    {(error || files.error) && <p role="alert">{error || errorMessage(files.error)}</p>}
    {files.data?.data.length === 0 && <p>{t("No attachments")}</p>}
    <ul>{files.data?.data.map((file) => <li key={file.id}>
      <button type="button" disabled={pending} onClick={() => void action(async () => triggerDownload(await fileApi.download(file.id), file.original_name))}>{file.original_name}</button>
      {writable && <button type="button" disabled={pending} onClick={() => void action(() => fileApi.detach(resource, recordId, file.id))}>{t("Detach")}</button>}
      {writable && <button type="button" disabled={pending} onClick={() => setReplacing(file.id)}>{t("Replace")}</button>}
    </li>)}</ul>
    {replacing && <p role="status">{t("Upload a replacement attachment")}<button type="button" onClick={() => setReplacing(null)}>{t("Cancel")}</button></p>}
    {writable && <FileUpload onUploaded={(file) => void action(async () => { if (replacing) await fileApi.replace(resource, recordId, replacing, file.id); else await fileApi.attach(resource, recordId, file.id); setReplacing(null); })} />}
  </section>;
}
