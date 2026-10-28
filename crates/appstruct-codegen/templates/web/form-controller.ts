import { t } from "./ui";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import {
  toApiValue,
  toFormValue,
  valueError,
  recordRuleErrors,
  type FormValue,
  type FormValues,
} from "./field-values";
import { resourceQueryKeys } from "./query";
import {
  canAccessRule,
  fieldErrors,
  useCanAccess,
  useResourceActor,
  type FieldDefinition,
  type ResourceDefinition,
  type ResourceInput,
  type ResourceRecord,
} from "./resource";

export interface ResourceFormControllerOptions {
  id?: string;
  initialRecord?: ResourceRecord;
  refetchRecord?(): Promise<ResourceRecord | undefined>;
  onSaved?(record: ResourceRecord): Promise<void> | void;
}

interface ConflictComparison {
  latestRecord: ResourceRecord;
  fields: {
    name: string;
    label: string;
    original: FormValue;
    mine: FormValue;
    latest: FormValue;
    draft: FormValue;
    mineOriginal: unknown;
    choice?: "mine" | "latest";
  }[];
}

export function useResourceFormController(
  resource: ResourceDefinition,
  options: ResourceFormControllerOptions = {},
) {
  const actor = useResourceActor();
  const queryClient = useQueryClient();
  const [baseline, setBaseline] = useState(options.initialRecord);
  const [inputOriginals, setInputOriginals] = useState(options.initialRecord);
  const editing = options.id !== undefined;
  const canSubmit = useCanAccess(
    resource,
    editing ? "update" : "create",
    baseline,
  );
  const [serverErrors, setServerErrors] = useState<Record<string, string>>({});
  const [conflict, setConflict] = useState(false);
  const [comparison, setComparison] = useState<ConflictComparison | null>(null);
  const [comparing, setComparing] = useState(false);
  const [comparisonError, setComparisonError] = useState<unknown>(null);
  const fields = useMemo(
    () =>
      resource.fields.filter(
        (field) =>
          !field.readOnly &&
          !field.primaryKey &&
          canAccessRule(field.writeAccess ?? { mode: "public" }, actor),
      ),
    [actor, resource],
  );
  const defaultValues = useMemo(
    () => recordFormValues(baseline, fields),
    [baseline, fields],
  );
  const saveMutation = useMutation({
    mutationFn: (input: ResourceInput) => {
      if (!canSubmit)
        throw new Error(
          t("You do not have permission to change this resource."),
        );
      return options.id
        ? resource.api.update(options.id, input, {
            revision: baseline?.revision as number | undefined,
          })
        : resource.api.create(input);
    },
    onSuccess: async (record) => {
      setBaseline(record);
      setInputOriginals(record);
      form.reset(recordFormValues(record, fields));
      queryClient.setQueryData(
        resourceQueryKeys.detail(
          resource.id,
          String(record[resource.primaryKey]),
        ),
        record,
      );
      await queryClient.invalidateQueries({
        queryKey: resourceQueryKeys.all(resource.id),
      });
      await options.onSaved?.(record);
    },
    onError: (reason) => {
      setServerErrors(fieldErrors(reason));
      setConflict(
        (reason as { code?: string } | undefined)?.code ===
          "CONCURRENT_MODIFICATION",
      );
      setComparison(null);
    },
  });
  const form = useForm({
    defaultValues,
    validators: {
      onChange: ({ value, formApi }) =>
        formApi.state.submissionAttempts > 0
          ? validateFormValues(
              value,
              fields,
              baseline,
              resource.fields,
              inputOriginals,
            )
          : undefined,
      onSubmit: ({ value }) =>
        validateFormValues(
          value,
          fields,
          baseline,
          resource.fields,
          inputOriginals,
        ),
    },
    onSubmit: async ({ value }) => {
      if (conflict) return;
      setServerErrors({});
      setConflict(false);
      saveMutation.reset();
      const entries = fields
        .filter(
          (field) =>
            editing ||
            field.required ||
            form.getFieldMeta(field.name)?.isTouched,
        )
        .map((field) => [
          field.name,
          toApiValue(value[field.name], field, inputOriginals?.[field.name]),
        ]);
      try {
        await saveMutation.mutateAsync(
          Object.fromEntries(entries) as ResourceInput,
        );
      } catch {
        /* Mutation state keeps submitted values and exposes the failure. */
      }
    },
  });

  async function reloadRecord() {
    if (!options.id) return;
    const record = options.refetchRecord
      ? await options.refetchRecord()
      : await resource.api.get(options.id);
    if (!record) return;
    setBaseline(record);
    setInputOriginals(record);
    form.reset(recordFormValues(record, fields));
    setServerErrors({});
    setConflict(false);
    setComparison(null);
    setComparisonError(null);
    saveMutation.reset();
  }

  async function compareConflict() {
    if (!options.id || comparing) return;
    setComparing(true);
    setComparisonError(null);
    try {
      const latestRecord = options.refetchRecord
        ? await options.refetchRecord()
        : await resource.api.get(options.id);
      if (!latestRecord)
        throw new Error(t("The latest record is unavailable."));
      const original = recordFormValues(baseline, fields);
      const latest = recordFormValues(latestRecord, fields);
      const mine = form.state.values;
      setComparison({
        latestRecord,
        fields: fields.map((field) => {
          const mineOriginal = toApiValue(
            mine[field.name],
            field,
            inputOriginals?.[field.name],
          );
          // Compare exact instants, even when native inputs cannot display micros.
          const before =
            field.kind === "datetime"
              ? String(baseline?.[field.name] ?? "")
              : original[field.name];
          const after =
            field.kind === "datetime"
              ? String(latestRecord[field.name] ?? "")
              : latest[field.name];
          const draft =
            field.kind === "datetime"
              ? String(mineOriginal ?? "")
              : mine[field.name];
          return {
            name: field.name,
            label: field.label,
            original: before,
            mine: draft,
            latest: after,
            draft: mine[field.name],
            mineOriginal,
            choice:
              draft === before
                ? "latest"
                : after === before || after === draft
                  ? "mine"
                  : undefined,
          };
        }),
      });
    } catch (error) {
      setComparisonError(error);
    } finally {
      setComparing(false);
    }
  }

  function chooseConflict(name: string, choice: "mine" | "latest") {
    setComparison(
      (current) =>
        current && {
          ...current,
          fields: current.fields.map((field) =>
            field.name === name ? { ...field, choice } : field,
          ),
        },
    );
  }

  function applyConflict() {
    if (!comparison || comparison.fields.some((field) => !field.choice)) return;
    setBaseline(comparison.latestRecord);
    setInputOriginals({
      ...comparison.latestRecord,
      ...Object.fromEntries(
        comparison.fields
          .filter((field) => field.choice === "mine")
          .map((field) => [field.name, field.mineOriginal]),
      ),
    });
    form.reset(recordFormValues(comparison.latestRecord, fields));
    for (const field of comparison.fields) {
      if (field.choice === "mine") form.setFieldValue(field.name, field.draft);
    }
    setConflict(false);
    setComparison(null);
    setServerErrors({});
    saveMutation.reset();
  }

  function clearServerError(field: string) {
    setServerErrors((current) => {
      if (!(field in current)) return current;
      const next = { ...current };
      delete next[field];
      return next;
    });
  }

  return {
    form,
    fields,
    canSubmit,
    serverErrors,
    conflict,
    error: saveMutation.error,
    saving: saveMutation.isPending,
    reloadRecord,
    clearServerError,
    comparison,
    comparing,
    comparisonError,
    compareConflict,
    chooseConflict,
    applyConflict,
  };
}

function recordFormValues(
  record: ResourceRecord | undefined,
  fields: FieldDefinition[],
): FormValues {
  return Object.fromEntries(
    fields.map((field) => [
      field.name,
      toFormValue(record?.[field.name], field),
    ]),
  );
}

function validateFormValues(
  values: FormValues,
  fields: FieldDefinition[],
  baseline?: ResourceRecord,
  allFields: FieldDefinition[] = fields,
  inputOriginals: ResourceRecord | undefined = baseline,
): { fields: Record<string, string> } | undefined {
  const errors: Record<string, string> = {};
  for (const field of fields) {
    const message = valueError(values[field.name], field);
    if (message) errors[field.name] = message;
  }
  const candidate = { ...baseline };
  for (const field of fields) {
    if (!errors[field.name])
      candidate[field.name] = toApiValue(
        values[field.name],
        field,
        inputOriginals?.[field.name],
      );
  }
  Object.assign(errors, recordRuleErrors(candidate, allFields));
  for (const field of fields) {
    if (field.semantic?.kind !== "money") continue;
    const amount = values[field.name];
    const currency = values[field.semantic.currencyField];
    if (Boolean(amount) === Boolean(currency)) continue;
    const missing = amount ? field.semantic.currencyField : field.name;
    errors[missing] ??= t("{0} and currency must be provided together", {
      "0": field.label,
    });
  }
  return Object.keys(errors).length ? { fields: errors } : undefined;
}
