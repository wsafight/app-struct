import { useMutation } from "@tanstack/react-query";
import { useRef } from "react";
import type { ResourceDefinition } from "../../resource";
import { errorMessage, triggerDownload } from "../../resource";
import type { ListQuery } from "../../generated/client";

interface CsvTransferOptions {
  resource: ResourceDefinition;
  runChange: (operation: () => Promise<void>) => Promise<boolean>;
  onError: (message: string) => void;
  query?: ListQuery;
}

export function useCsvTransfer({
  resource,
  runChange,
  onError,
  query = {},
}: CsvTransferOptions) {
  const importInput = useRef<HTMLInputElement>(null);
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
      await runChange(async () => {
        const result = await resource.api.importCsv(await file.text());
        if (result.failed.length)
          onError(`${result.failed.length} rows could not be imported`);
      });
    } finally {
      if (importInput.current) importInput.current.value = "";
    }
  }

  return {
    importInput,
    exporting: exportMutation.isPending,
    exportCsv: () => exportMutation.mutate(),
    importCsv,
  };
}
