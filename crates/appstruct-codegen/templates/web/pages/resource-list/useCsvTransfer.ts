import { useMutation } from "@tanstack/react-query";
import { useRef } from "react";
import type { ResourceDefinition } from "../../resource";
import { errorMessage } from "../../resource";

interface CsvTransferOptions {
  resource: ResourceDefinition;
  runChange: (operation: () => Promise<void>) => Promise<boolean>;
  onError: (message: string) => void;
}

export function useCsvTransfer({
  resource,
  runChange,
  onError,
}: CsvTransferOptions) {
  const importInput = useRef<HTMLInputElement>(null);
  const exportMutation = useMutation({
    mutationFn: () => resource.api.exportCsv(),
    onSuccess: (csv) => {
      const href = URL.createObjectURL(
        new Blob([csv], { type: "text/csv;charset=utf-8" }),
      );
      const anchor = document.createElement("a");
      anchor.href = href;
      anchor.download = `${resource.slug}.csv`;
      anchor.click();
      URL.revokeObjectURL(href);
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
