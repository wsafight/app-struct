import { formatDateTime, formatNumber } from "../ui";
import { t } from "../ui";
import { useQuery } from "@tanstack/react-query";
import { type FormEvent, type ReactNode, useState } from "react";
import { ArrowLeft, Eye, Search, X } from "lucide-react";
import {
  adminApi,
  adminFeatures,
  type AdminFile,
  type AdminMailDelivery,
  type AdminMailSummary,
} from "../generated/client";
import { Link, Navigate, useParams } from "../navigation";
import { appQueryKeys } from "../query";
import { errorMessage } from "../resource";
import { AdminPagination } from "./AuthPages";
import { useAuth } from "./Auth";

const PAGE_SIZE = 25;

export function AdminMailPage() {
  const auth = useAuth();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const [draftSearch, setDraftSearch] = useState("");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const mailQuery = useQuery({
    queryKey: appQueryKeys.admin.mail(search, page, PAGE_SIZE),
    queryFn: ({ signal }) =>
      adminApi.listMail(
        { search: search || undefined, page, page_size: PAGE_SIZE },
        { signal },
      ),
    enabled: adminFeatures.mail && isAdmin,
    placeholderData: (previous) => previous,
  });
  if (!isAdmin || !adminFeatures.mail) return <Navigate to="/admin" replace />;
  const deliveries: AdminMailSummary[] = mailQuery.data?.data ?? [];
  const error = mailQuery.error ? errorMessage(mailQuery.error) : "";

  function submitSearch(event: FormEvent) {
    event.preventDefault();
    setSearch(draftSearch.trim());
    setPage(1);
  }

  function clearSearch() {
    setDraftSearch("");
    setSearch("");
    setPage(1);
  }

  return (
    <main className="page">
      <AdminBackLink />
      <div className="page-heading storage-heading">
        <div>
          <h1> {t("Mail deliveries")}</h1>
          <p>
            {mailQuery.data ? formatNumber(mailQuery.data.meta.total) : 0}{" "}
            {t("captured")}
          </p>
        </div>
        <StorageSearch
          label={t("Search mail")}
          value={draftSearch}
          active={Boolean(search)}
          onChange={setDraftSearch}
          onSubmit={submitSearch}
          onClear={clearSearch}
        />
      </div>
      <QueryError message={error} />
      <section className="table-frame admin-storage-table">
        <table>
          <thead>
            <tr>
              <th>{t("Recipient")}</th>
              <th> {t("Subject")}</th>
              <th> {t("Template")} </th>
              <th>{t("Provider")}</th>
              <th> {t("Tenant")}</th>
              <th> {t("Created")} </th>
              <th aria-label={t("Actions")} />
            </tr>
          </thead>
          <tbody>
            {deliveries.map((delivery) => (
              <tr key={delivery.id}>
                <td title={delivery.recipient}>{delivery.recipient}</td>
                <td title={delivery.subject}>
                  <Link to={`/admin/mail/${delivery.id}`}>
                    {delivery.subject}
                  </Link>
                </td>
                <td>{delivery.template}</td>
                <td>{delivery.provider}</td>
                <td title={delivery.tenant_id ?? undefined}>
                  {shortId(delivery.tenant_id)}
                </td>
                <td>{formatDate(delivery.created_at)}</td>
                <td>
                  <Link
                    className="icon-button icon-link"
                    to={`/admin/mail/${delivery.id}`}
                    title={t("View delivery")}
                    aria-label={t("View mail to {0}", {
                      "0": delivery.recipient,
                    })}
                  >
                    <Eye size={15} />
                  </Link>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {deliveries.length === 0 && !mailQuery.isPending && (
          <div className="empty"> {t("No mail deliveries found")}</div>
        )}
      </section>
      <AdminPagination
        page={page}
        pageSize={PAGE_SIZE}
        total={mailQuery.data?.meta.total ?? 0}
        onPageChange={setPage}
      />
    </main>
  );
}

export function AdminMailDetailPage() {
  const auth = useAuth();
  const { id } = useParams();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const mailQuery = useQuery({
    queryKey: appQueryKeys.admin.mailDetail(id ?? ""),
    queryFn: ({ signal }) => adminApi.getMail(id ?? "", { signal }),
    enabled: adminFeatures.mail && isAdmin && Boolean(id),
  });
  if (!isAdmin || !adminFeatures.mail) return <Navigate to="/admin" replace />;
  if (!id) return <Navigate to="/admin/mail" replace />;
  const delivery: AdminMailDelivery | undefined = mailQuery.data;
  const error = mailQuery.error ? errorMessage(mailQuery.error) : "";
  return (
    <main className="page detail-page">
      <Link className="back-link" to="/admin/mail">
        <ArrowLeft size={15} /> {t("Mail deliveries")}
      </Link>
      <div className="page-heading">
        <div>
          <h1>{delivery?.subject ?? "Mail delivery"}</h1>
          {delivery && <p>{formatDate(delivery.created_at)}</p>}
        </div>
      </div>
      <QueryError message={error} />
      {!delivery && !error && (
        <div className="auth-loading" aria-label={t("Loading")} />
      )}
      {delivery && (
        <>
          <dl className="storage-detail-grid">
            <Detail label={t("Recipient")} value={delivery.recipient} />
            <Detail label={t("Sender")} value={delivery.sender} />
            <Detail label={t("Template")} value={delivery.template} />
            <Detail label={t("Provider")} value={delivery.provider} />
            <Detail
              label={t("Tenant")}
              value={delivery.tenant_id ?? "Global"}
            />
            <Detail label={t("Delivery ID")} value={delivery.id} mono />
          </dl>
          <section className="storage-body-section">
            <h2>{t("Text body")}</h2>
            <pre>{delivery.text_body || "(empty)"}</pre>
          </section>
          {delivery.html_body && (
            <section className="storage-body-section">
              <h2>{t("HTML source")} </h2>
              <pre>{delivery.html_body}</pre>
            </section>
          )}
        </>
      )}
    </main>
  );
}

export function AdminFilesPage() {
  const auth = useAuth();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const [draftSearch, setDraftSearch] = useState("");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const filesQuery = useQuery({
    queryKey: appQueryKeys.admin.files(search, page, PAGE_SIZE),
    queryFn: ({ signal }) =>
      adminApi.listFiles(
        { search: search || undefined, page, page_size: PAGE_SIZE },
        { signal },
      ),
    enabled: adminFeatures.file && isAdmin,
    placeholderData: (previous) => previous,
  });
  if (!isAdmin || !adminFeatures.file) return <Navigate to="/admin" replace />;
  const files: AdminFile[] = filesQuery.data?.data ?? [];
  const error = filesQuery.error ? errorMessage(filesQuery.error) : "";

  function submitSearch(event: FormEvent) {
    event.preventDefault();
    setSearch(draftSearch.trim());
    setPage(1);
  }

  function clearSearch() {
    setDraftSearch("");
    setSearch("");
    setPage(1);
  }

  return (
    <main className="page">
      <AdminBackLink />
      <div className="page-heading storage-heading">
        <div>
          <h1>{t("Files")}</h1>
          <p>
            {filesQuery.data ? formatNumber(filesQuery.data.meta.total) : 0}{" "}
            {t("objects,")} {formatBytes(filesQuery.data?.total_bytes ?? 0)}
          </p>
        </div>
        <StorageSearch
          label={t("Search files")}
          value={draftSearch}
          active={Boolean(search)}
          onChange={setDraftSearch}
          onSubmit={submitSearch}
          onClear={clearSearch}
        />
      </div>
      <QueryError message={error} />
      <section className="table-frame admin-storage-table">
        <table>
          <thead>
            <tr>
              <th>{t("Name")}</th>
              <th>{t("Object key")}</th>
              <th>{t("Content type")}</th>
              <th>{t("Size")}</th>
              <th> {t("Tenant")}</th>
              <th> {t("Created")} </th>
              <th aria-label={t("Actions")} />
            </tr>
          </thead>
          <tbody>
            {files.map((file) => (
              <tr key={file.id}>
                <td>
                  <Link to={`/admin/files/${file.id}`}>
                    {file.original_name}
                  </Link>
                </td>
                <td title={file.object_key} className="storage-key">
                  {file.object_key}
                </td>
                <td>{file.content_type}</td>
                <td>{formatBytes(file.size)}</td>
                <td title={file.tenant_id ?? undefined}>
                  {shortId(file.tenant_id)}
                </td>
                <td>{formatDate(file.created_at)}</td>
                <td>
                  <Link
                    className="icon-button icon-link"
                    to={`/admin/files/${file.id}`}
                    title={t("View file metadata")}
                    aria-label={t("View {0}", { "0": file.original_name })}
                  >
                    <Eye size={15} />
                  </Link>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {files.length === 0 && !filesQuery.isPending && (
          <div className="empty">{t("No files found")}</div>
        )}
      </section>
      <AdminPagination
        page={page}
        pageSize={PAGE_SIZE}
        total={filesQuery.data?.meta.total ?? 0}
        onPageChange={setPage}
      />
    </main>
  );
}

export function AdminFileDetailPage() {
  const auth = useAuth();
  const { id } = useParams();
  const isAdmin = auth.user?.roles.includes("admin") ?? false;
  const fileQuery = useQuery({
    queryKey: appQueryKeys.admin.fileDetail(id ?? ""),
    queryFn: ({ signal }) => adminApi.getFile(id ?? "", { signal }),
    enabled: adminFeatures.file && isAdmin && Boolean(id),
  });
  if (!isAdmin || !adminFeatures.file) return <Navigate to="/admin" replace />;
  if (!id) return <Navigate to="/admin/files" replace />;
  const file: AdminFile | undefined = fileQuery.data;
  const error = fileQuery.error ? errorMessage(fileQuery.error) : "";
  return (
    <main className="page detail-page">
      <Link className="back-link" to="/admin/files">
        <ArrowLeft size={15} /> {t("Files")}
      </Link>
      <div className="page-heading">
        <div>
          <h1>{file?.original_name ?? "File metadata"}</h1>
          {file && <p>{formatBytes(file.size)}</p>}
        </div>
      </div>
      <QueryError message={error} />
      {!file && !error && (
        <div className="auth-loading" aria-label={t("Loading")} />
      )}
      {file && (
        <dl className="storage-detail-grid">
          <Detail label={t("Object key")} value={file.object_key} mono wide />
          <Detail label={t("Content type")} value={file.content_type} />
          <Detail
            label={t("Size")}
            value={t("{0} bytes", { 0: formatNumber(file.size) })}
          />
          <Detail label={t("Tenant")} value={file.tenant_id ?? t("Global")} />
          <Detail label={t("Created")} value={formatDate(file.created_at)} />
          <Detail label={t("File ID")} value={file.id} mono />
          <Detail label="SHA-256" value={file.checksum} mono wide />
        </dl>
      )}
    </main>
  );
}

function AdminBackLink() {
  return (
    <Link className="back-link" to="/admin">
      <ArrowLeft size={15} /> {t("Administration")}
    </Link>
  );
}

function StorageSearch({
  label,
  value,
  active,
  onChange,
  onSubmit,
  onClear,
}: {
  label: string;
  value: string;
  active: boolean;
  onChange(value: string): void;
  onSubmit(event: FormEvent): void;
  onClear(): void;
}) {
  return (
    <form className="storage-search" role="search" onSubmit={onSubmit}>
      <input
        aria-label={label}
        placeholder={label}
        maxLength={200}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      <button
        className="icon-button"
        type="submit"
        title={t("Search")}
        aria-label={t("Search")}
      >
        <Search size={16} />
      </button>
      {(active || value) && (
        <button
          className="icon-button"
          type="button"
          title={t("Clear search")}
          aria-label={t("Clear search")}
          onClick={onClear}
        >
          <X size={16} />
        </button>
      )}
    </form>
  );
}

function Detail({
  label,
  value,
  mono = false,
  wide = false,
}: {
  label: string;
  value: ReactNode;
  mono?: boolean;
  wide?: boolean;
}) {
  return (
    <div className={wide ? "wide" : undefined}>
      <dt>{label}</dt>
      <dd className={mono ? "storage-mono" : undefined}>{value}</dd>
    </div>
  );
}

function QueryError({ message }: { message: string }) {
  return message ? (
    <div className="alert" role="alert">
      {message}
    </div>
  ) : null;
}

function shortId(value: string | null): string {
  return value ? `${value.slice(0, 8)}...` : t("Global");
}

function formatDate(value: string): string {
  return formatDateTime(value);
}

function formatBytes(value: number): string {
  if (value < 1024) return `${formatNumber(value)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let size = value / 1024;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${formatNumber(size, { maximumFractionDigits: 1 })} ${units[unit]}`;
}
