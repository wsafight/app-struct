import { t } from "../../ui";
import { useMutation } from "@tanstack/react-query";
import { useRef, useState } from "react";
import type { ResourceDefinition } from "../../resource";
import { errorMessage, triggerDownload } from "../../resource";
import type { ListQuery } from "../../generated/client";
import { MAX_IMPORT_BYTES } from "./csv-import";
import type { CsvImportFile } from "./CsvImportDialog";

interface CsvTransferOptions {
  resource: ResourceDefinition;
  runChange: (operation: () => Promise<void>) => Promise<boolean>;
  onError: (message: string) => void;
  query?: ListQuery;
}

export function useCsvTransfer({
  resource,
  onError,
  query = {},
}: CsvTransferOptions) {
  const importInput = useRef<HTMLInputElement>(null);
  const [importFile, setImportFile] = useState<CsvImportFile | null>(null);
  const exportMutation = useMutation({
    mutationFn: async () => {
      const filters = {
        q: query.q,
        where: query.where,
        filters: query.filters,
        range_filters: query.range_filters,
        sort: query.sort,
        fields: query.fields,
      };
      if (resource.api.downloadCsv) {
        await resource.api.downloadCsv(filters);
        return;
      }
      return resource.api.exportCsv(filters);
    },
    onSuccess: (csv) => {
      if (csv === undefined) return;
      triggerDownload(
        new Blob([csv], { type: "text/csv;charset=utf-8" }),
        `${resource.slug}.csv`,
      );
    },
    onError: (reason) => onError(errorMessage(reason)),
  });

  async function importCsv(file?: File) {
    if (!file) return;
    try {
      if (file.size > MAX_IMPORT_BYTES)
        throw new Error(t("CSV import is limited to 2 MiB."));
      setImportFile({ name: file.name, text: await file.text() });
      onError("");
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      if (importInput.current) importInput.current.value = "";
    }
  }

  return {
    importInput,
    importFile,
    closeImport: () => setImportFile(null),
    exporting: exportMutation.isPending,
    exportCsv: () => exportMutation.mutate(),
    importCsv,
  };
}
