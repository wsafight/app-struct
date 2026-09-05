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
          <h1>Mail deliveries</h1>
          <p>{mailQuery.data?.meta.total.toLocaleString() ?? 0} captured</p>
        </div>
        <StorageSearch
          label="Search mail"
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
              <th>Recipient</th>
              <th>Subject</th>
              <th>Template</th>
              <th>Provider</th>
              <th>Tenant</th>
              <th>Created</th>
              <th aria-label="Actions" />
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
                    title="View delivery"
                    aria-label={`View mail to ${delivery.recipient}`}
                  >
                    <Eye size={15} />
                  </Link>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {deliveries.length === 0 && !mailQuery.isPending && (
          <div className="empty">No mail deliveries found</div>
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
        <ArrowLeft size={15} /> Mail deliveries
      </Link>
      <div className="page-heading">
        <div>
          <h1>{delivery?.subject ?? "Mail delivery"}</h1>
          {delivery && <p>{formatDate(delivery.created_at)}</p>}
        </div>
      </div>
      <QueryError message={error} />
      {!delivery && !error && (
        <div className="auth-loading" aria-label="Loading" />
      )}
      {delivery && (
        <>
          <dl className="storage-detail-grid">
            <Detail label="Recipient" value={delivery.recipient} />
            <Detail label="Sender" value={delivery.sender} />
            <Detail label="Template" value={delivery.template} />
            <Detail label="Provider" value={delivery.provider} />
            <Detail label="Tenant" value={delivery.tenant_id ?? "Global"} />
            <Detail label="Delivery ID" value={delivery.id} mono />
          </dl>
          <section className="storage-body-section">
            <h2>Text body</h2>
            <pre>{delivery.text_body || "(empty)"}</pre>
          </section>
          {delivery.html_body && (
            <section className="storage-body-section">
              <h2>HTML source</h2>
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
          <h1>Files</h1>
          <p>
            {filesQuery.data?.meta.total.toLocaleString() ?? 0} objects,{" "}
            {formatBytes(filesQuery.data?.total_bytes ?? 0)}
          </p>
        </div>
        <StorageSearch
          label="Search files"
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
              <th>Name</th>
              <th>Object key</th>
              <th>Content type</th>
              <th>Size</th>
              <th>Tenant</th>
              <th>Created</th>
              <th aria-label="Actions" />
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
                    title="View file metadata"
                    aria-label={`View ${file.original_name}`}
                  >
                    <Eye size={15} />
                  </Link>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {files.length === 0 && !filesQuery.isPending && (
          <div className="empty">No files found</div>
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
        <ArrowLeft size={15} /> Files
      </Link>
      <div className="page-heading">
        <div>
          <h1>{file?.original_name ?? "File metadata"}</h1>
          {file && <p>{formatBytes(file.size)}</p>}
        </div>
      </div>
      <QueryError message={error} />
      {!file && !error && <div className="auth-loading" aria-label="Loading" />}
      {file && (
        <dl className="storage-detail-grid">
          <Detail label="Object key" value={file.object_key} mono wide />
          <Detail label="Content type" value={file.content_type} />
          <Detail label="Size" value={`${file.size.toLocaleString()} bytes`} />
          <Detail label="Tenant" value={file.tenant_id ?? "Global"} />
          <Detail label="Created" value={formatDate(file.created_at)} />
          <Detail label="File ID" value={file.id} mono />
          <Detail label="SHA-256" value={file.checksum} mono wide />
        </dl>
      )}
    </main>
  );
}

function AdminBackLink() {
  return (
    <Link className="back-link" to="/admin">
      <ArrowLeft size={15} /> Administration
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
        title="Search"
        aria-label="Search"
      >
        <Search size={16} />
      </button>
      {(active || value) && (
        <button
          className="icon-button"
          type="button"
          title="Clear search"
          aria-label="Clear search"
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
  return value ? `${value.slice(0, 8)}...` : "Global";
}

function formatDate(value: string): string {
  return new Date(value).toLocaleString();
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value.toLocaleString()} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let size = value / 1024;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toLocaleString(undefined, { maximumFractionDigits: 1 })} ${units[unit]}`;
}
