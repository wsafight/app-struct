import { t } from "../../ui";
import { useMemo, useRef, useState } from "react";
import { toApiValue, recordRuleErrors } from "../../field-values";
import { DialogFrame } from "../../components/Dialog";
import { ApiError, type BulkResult } from "../../generated/client";
import {
  canAccessRule,
  errorMessage,
  triggerDownload,
  useResourceActor,
  type ResourceDefinition,
} from "../../resource";
import {
  csvValueError,
  MAX_IMPORT_BYTES,
  parseCsv,
  writeCsv,
} from "./csv-import";

export interface CsvImportFile {
  name: string;
  text: string;
}
interface ImportRow {
  source: number;
  values: string[];
  error?: string;
}

export function CsvImportDialog({
  resource,
  file,
  runChange,
  onError,
  onClose,
}: {
  resource: ResourceDefinition;
  file: CsvImportFile;
  runChange(operation: () => Promise<void>): Promise<boolean>;
  onError(message: string): void;
  onClose(): void;
}) {
  const actor = useResourceActor();
  const fields = resource.fields.filter(
    (field) =>
      !field.readOnly &&
      canAccessRule(field.writeAccess ?? { mode: "public" }, actor),
  );
  const parsed = useMemo(() => {
    try {
      return { rows: parseCsv(file.text), error: "" };
    } catch (error) {
      return { rows: [] as string[][], error: errorMessage(error) };
    }
  }, [file.text]);
  const headers = parsed.rows[0] ?? [];
  const [mapping, setMapping] = useState(() =>
    headers.map(
      (name) =>
        fields.find((field) =>
          [field.name, field.apiName].some(
            (candidate) => candidate.toLowerCase() === name.toLowerCase(),
          ),
        )?.apiName ?? "",
    ),
  );
  const [rows, setRows] = useState<ImportRow[]>(() =>
    parsed.rows
      .slice(1)
      .map((values, index) => ({ source: index + 2, values })),
  );
  const [succeeded, setSucceeded] = useState(0);
  const [attempted, setAttempted] = useState(false);
  const [busy, setBusy] = useState(false);
  const attempt = useRef<{ batch: string; csv: string } | null>(null);
  const [unresolved, setUnresolved] = useState(false);
  const [page, setPage] = useState(1);
  const mapped = mapping.flatMap((name, column) => {
    const field = fields.find((field) => field.apiName === name);
    return field ? [{ column, field }] : [];
  });
  const missing = fields.filter(
    (field) => field.required && !mapping.includes(field.apiName),
  );
  const mappingError = !mapped.length
    ? t("Choose at least one field.")
    : new Set(mapping.filter(Boolean)).size !== mapped.length
      ? t("Map each field only once.")
      : missing.length
        ? t("Required fields are missing: {0}", {
            "0": missing.map((field) => field.label).join(", "),
          })
        : "";
  const issues = rows.map((row) => {
    if (row.values.length !== headers.length)
      return t("This row has a different number of columns than the header.");
    const errors = mapped
      .map(({ column, field }) => csvValueError(row.values[column], field))
      .filter(Boolean);
    if (errors.length) return errors.join("; ");
    const candidate = Object.fromEntries(
      mapped.map(({ column, field }) => {
        const value = row.values[column];
        return [
          field.name,
          field.kind === "datetime" && /(?:Z|[+-]\d{2}:\d{2})$/.test(value)
            ? value
            : toApiValue(
                field.kind === "boolean" && value ? value === "true" : value,
                field,
              ),
        ];
      }),
    );
    return Object.values(recordRuleErrors(candidate, resource.fields)).join(
      "; ",
    );
  });
  const invalid = issues.filter(Boolean).length;
  const pages = Math.max(1, Math.ceil(rows.length / 25));
  const currentPage = Math.min(page, pages);
  const visibleRows = rows.slice((currentPage - 1) * 25, currentPage * 25);

  async function submit() {
    if (busy || mappingError || invalid || !rows.length) return;
    const csv = writeCsv([
      mapped.map(({ field }) => field.apiName),
      ...rows.map((row) =>
        mapped.map(({ column, field }) => {
          const value = row.values[column];
          // CSV dates with an explicit timezone already use the API representation.
          if (
            field.kind === "datetime" &&
            value &&
            !/(?:Z|[+-]\d{2}:\d{2})$/.test(value)
          )
            return String(toApiValue(value, field));
          return value;
        }),
      ),
    ]);
    if (new Blob([csv]).size > MAX_IMPORT_BYTES) {
      onError(t("CSV import is limited to 2 MiB."));
      return;
    }
    setBusy(true);
    try {
      let result: BulkResult | undefined;
      const changed = await runChange(async () => {
        const firstRequest = attempt.current === null;
        attempt.current ??= { batch: crypto.randomUUID(), csv };
        setUnresolved(true);
        try {
          result = await resource.api.importCsv(
            attempt.current.csv,
            attempt.current.batch,
          );
        } catch (error) {
          // An authoritative rejection of the first attempt cannot have committed rows.
          if (
            firstRequest &&
            error instanceof ApiError &&
            [400, 401, 403, 404, 413, 422, 429].includes(error.status)
          ) {
            attempt.current = null;
            setUnresolved(false);
          }
          throw error;
        }
      });
      if (!changed || !result) return;
      acceptResult(result);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  function acceptResult(result: BulkResult) {
    attempt.current = null;
    setUnresolved(false);
    const failures = new Map(
      result.failed.map((failure) => [
        Number(failure.id),
        errorMessage(failure),
      ]),
    );
    const successIds = new Set(result.succeeded.map(Number));
    setSucceeded((count) => count + successIds.size);
    setRows(
      rows.flatMap((row, index) =>
        successIds.has(index)
          ? []
          : [
              {
                ...row,
                error:
                  failures.get(index) ??
                  t("No result was returned for this row."),
              },
            ],
      ),
    );
    setAttempted(true);
    setPage(1);
  }

  async function checkResult() {
    if (!attempt.current || busy) return;
    setBusy(true);
    try {
      let result: BulkResult | undefined;
      const batch = attempt.current.batch;
      const changed = await runChange(async () => {
        result = await resource.api.importResult(batch);
      });
      if (changed && result) {
        acceptResult(result);
        onError("");
      }
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  function downloadFailures() {
    const csv = writeCsv([headers, ...rows.map((row) => row.values)]);
    triggerDownload(
      new Blob([csv], { type: "text/csv;charset=utf-8" }),
      `failed-${file.name}`,
    );
  }

  return (
    <DialogFrame
      open
      title={t("Import {0}", { "0": resource.label })}
      onCancel={() => {
        if (!busy) onClose();
      }}
    >
      <p>
        {file.name} · {succeeded} {t("imported ·")}
        {rows.length} {attempted ? t("remaining") : t("rows to import")}
      </p>
      {unresolved && attempt.current && (
        <div className="alert" role="status">
          <p>
            {t("Import result is not confirmed. Keep batch ID:")}
            <code>{attempt.current.batch}</code>
          </p>
          <p>
            {t(
              "Check the committed result or retry this same batch. Editing is paused until the result is confirmed.",
            )}
          </p>
          <button
            type="button"
            className="secondary-button"
            disabled={busy}
            onClick={() => void checkResult()}
          >
            {t("Check import result")}
          </button>
        </div>
      )}
      {parsed.error && <p role="alert">{parsed.error}</p>}
      {!parsed.error && rows.length > 0 && (
        <>
          <div className="import-mapping">
            {headers.map((header, index) => (
              <label key={header}>
                {header}
                <select
                  aria-label={t("Map {0}", { "0": header })}
                  disabled={busy || attempted || unresolved}
                  value={mapping[index]}
                  onChange={(event) => {
                    setMapping(
                      mapping.map((name, column) =>
                        column === index ? event.target.value : name,
                      ),
                    );
                  }}
                >
                  <option value=""> {t("Skip column")} </option>
                  {fields.map((field) => (
                    <option key={field.name} value={field.apiName}>
                      {field.label}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </div>
          {mappingError && <p role="alert">{mappingError}</p>}
          {invalid > 0 && (
            <p role="alert">
              {invalid} {t("rows need correction before import.")}
            </p>
          )}
          <div className="table-frame import-preview">
            <table>
              <thead>
                <tr>
                  <th>{t("CSV row")}</th>
                  {mapped.map(({ field }) => (
                    <th key={field.name}>{field.label}</th>
                  ))}
                  <th> {t("Result")}</th>
                </tr>
              </thead>
              <tbody>
                {visibleRows.map((row, position) => (
                  <tr key={row.source}>
                    <td>{row.source}</td>
                    {mapped.map(({ column, field }) => (
                      <td key={field.name}>
                        <input
                          aria-label={t("Row {0} {1}", {
                            "0": row.source,
                            "1": field.label,
                          })}
                          disabled={busy || unresolved}
                          value={row.values[column] ?? ""}
                          onChange={(event) => {
                            setRows(
                              rows.map((item) =>
                                item.source !== row.source
                                  ? item
                                  : {
                                      ...item,
                                      error: undefined,
                                      values: Array.from(
                                        {
                                          length: Math.max(
                                            item.values.length,
                                            column + 1,
                                          ),
                                        },
                                        (_, index) =>
                                          index === column
                                            ? event.target.value
                                            : (item.values[index] ?? ""),
                                      ),
                                    },
                              ),
                            );
                          }}
                        />
                      </td>
                    ))}
                    <td>
                      {issues[(currentPage - 1) * 25 + position] ||
                        row.error ||
                        "Ready"}
                      {row.values.length !== headers.length && (
                        <button
                          type="button"
                          className="secondary-button"
                          disabled={busy}
                          onClick={() =>
                            setRows(
                              rows.map((item) =>
                                item.source !== row.source
                                  ? item
                                  : {
                                      ...item,
                                      values: headers.map(
                                        (_, index) => item.values[index] ?? "",
                                      ),
                                      error: undefined,
                                    },
                              ),
                            )
                          }
                        >
                          {t("Trim or fill columns to match header")}
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="pagination">
            <span>
              {" "}
              {t("Preview page")} {currentPage} {t("of")} {pages}
            </span>
            <button
              type="button"
              disabled={busy || currentPage === 1}
              onClick={() => setPage(currentPage - 1)}
            >
              {" "}
              {t("Previous preview")}
            </button>
            <button
              type="button"
              disabled={busy || currentPage === pages}
              onClick={() => setPage(currentPage + 1)}
            >
              {" "}
              {t("Next preview")}
            </button>
          </div>
        </>
      )}
      {!parsed.error && !rows.length && (
        <p role="status">
          {succeeded
            ? t("All rows were imported.")
            : t("CSV contains no data rows.")}
        </p>
      )}
      <div className="dialog-actions">
        {attempted && rows.length > 0 && (
          <button
            type="button"
            className="secondary-button"
            disabled={busy}
            onClick={downloadFailures}
          >
            {t("Download failed rows")}
          </button>
        )}
        <button
          type="button"
          className="secondary-button"
          disabled={busy}
          onClick={onClose}
        >
          {t("Close")}
        </button>
        {rows.length > 0 && (
          <button
            type="button"
            className="primary-button"
            disabled={
              busy || Boolean(parsed.error || mappingError) || invalid > 0
            }
            onClick={() => void submit()}
          >
            {busy
              ? "Importing..."
              : unresolved
                ? t("Retry same batch")
                : attempted
                  ? t("Retry remaining rows")
                  : t("Import {0} rows", { "0": rows.length })}
          </button>
        )}
      </div>
    </DialogFrame>
  );
}
