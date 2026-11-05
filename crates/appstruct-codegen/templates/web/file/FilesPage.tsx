import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { fileApi, fileFeatures, type UserFile } from "../generated/client";
import { ConfirmDialog } from "../components/Dialog";
import { errorMessage, triggerDownload } from "../resource";
import { formatDateTime, t } from "../ui";

export function FileUpload({
  onUploaded,
}: {
  onUploaded?: (file: UserFile) => void;
}) {
  const client = useQueryClient();
  const controller = useRef<AbortController | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const [selected, setSelected] = useState<File | null>(null);
  const [pending, setPending] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState("");
  useEffect(() => () => controller.current?.abort(), []);
  async function upload() {
    if (!selected) return;
    setPending(true);
    setProgress(0);
    setError("");
    controller.current = new AbortController();
    try {
      const file = await fileApi.upload(
        selected,
        setProgress,
        controller.current.signal,
      );
      await client.invalidateQueries({ queryKey: ["files"] });
      onUploaded?.(file);
      setSelected(null);
      if (input.current) input.current.value = "";
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
      controller.current = null;
    }
  }
  return (
    <section className="panel file-upload">
      <h2>{t("Upload a file")}</h2>
      <p>
        {t("Maximum file size: {0} MB", {
          "0": Math.floor(fileFeatures.maxBytes / 1024 / 1024),
        })}
      </p>
      <label>
        {t("File")}
        <input
          ref={input}
          type="file"
          accept={fileFeatures.allowedContentTypes.join(",")}
          disabled={pending}
          onChange={(event) => {
            setSelected(event.target.files?.[0] ?? null);
            setError("");
          }}
        />
      </label>
      <button
        className="primary-button"
        type="button"
        disabled={!selected || pending}
        onClick={() => void upload()}
      >
        {t(pending ? "Uploading..." : "Upload")}
      </button>
      {pending && (
        <>
          <progress
            max={100}
            value={progress}
            aria-label={t("Upload progress")}
          />
          <button
            type="button"
            className="secondary-button"
            onClick={() => controller.current?.abort()}
          >
            {t("Cancel")}
          </button>
        </>
      )}
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
    </section>
  );
}

export function FilesPage() {
  const client = useQueryClient();
  const [page, setPage] = useState(1);
  const [error, setError] = useState("");
  const [deleting, setDeleting] = useState<UserFile | null>(null);
  const query = useQuery({
    queryKey: ["files", page],
    queryFn: ({ signal }) => fileApi.list(page, 25, { signal }),
  });
  async function download(file: UserFile) {
    setError("");
    try {
      triggerDownload(await fileApi.download(file.id), file.original_name);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }
  return (
    <main className="page files-page">
      <h1>{t("Files")}</h1>
      <p>{t("Files shared in your current workspace.")}</p>
      <FileUpload onUploaded={() => setPage(1)} />
      {(error || query.error) && (
        <div className="alert" role="alert">
          {error || errorMessage(query.error)}
        </div>
      )}
      {query.isPending ? (
        <p role="status">{t("Loading...")}</p>
      ) : (
        <section className="table-frame">
          <table>
            <thead>
              <tr>
                <th>{t("Name")}</th>
                <th>{t("Size")}</th>
                <th>{t("Created")}</th>
                <th>{t("Actions")}</th>
              </tr>
            </thead>
            <tbody>
              {query.data?.data.map((file) => (
                <tr key={file.id}>
                  <td>{file.original_name}</td>
                  <td>{Math.ceil(file.size / 1024)} KB</td>
                  <td>{formatDateTime(file.created_at)}</td>
                  <td>
                    <button
                      type="button"
                      className="secondary-button"
                      onClick={() => void download(file)}
                    >
                      {t("Download")}
                    </button>
                    <button
                      type="button"
                      className="secondary-button"
                      onClick={() => setDeleting(file)}
                    >
                      {t("Delete")}
                    </button>
                  </td>
                </tr>
              ))}
              {query.data?.data.length === 0 && (
                <tr>
                  <td colSpan={4}>
                    {t("No files yet. Upload your first file.")}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </section>
      )}
      <div className="file-pagination">
        <button
          type="button"
          disabled={page <= 1}
          onClick={() => setPage(page - 1)}
        >
          {t("Previous page")}
        </button>
        <span>{page}</span>
        <button
          type="button"
          disabled={!query.data || page * 25 >= query.data.meta.total}
          onClick={() => setPage(page + 1)}
        >
          {t("Next page")}
        </button>
      </div>
      <ConfirmDialog
        open={Boolean(deleting)}
        title={t("Delete file")}
        description={deleting?.original_name ?? ""}
        confirmLabel={t("Delete")}
        danger
        onCancel={() => setDeleting(null)}
        onConfirm={async () => {
          if (!deleting) return;
          try {
            await fileApi.remove(deleting.id);
            setDeleting(null);
            setError("");
            await client.invalidateQueries({ queryKey: ["files"] });
          } catch (reason) {
            setError(errorMessage(reason));
            setDeleting(null);
          }
        }}
      />
    </main>
  );
}
