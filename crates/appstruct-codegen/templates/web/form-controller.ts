import { useForm } from "@tanstack/react-form";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { z } from "zod";
import {
  fieldSchema,
  toApiValue,
  toFormValue,
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

export function useResourceFormController(
  resource: ResourceDefinition,
  options: ResourceFormControllerOptions = {},
) {
  const actor = useResourceActor();
  const queryClient = useQueryClient();
  const [baseline, setBaseline] = useState(options.initialRecord);
  const editing = options.id !== undefined;
  const canSubmit = useCanAccess(
    resource,
    editing ? "update" : "create",
    baseline,
  );
  const [serverErrors, setServerErrors] = useState<Record<string, string>>({});
  const [conflict, setConflict] = useState(false);
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
  const validationSchema = useMemo(
    () => buildValidationSchema(fields),
    [fields],
  );
  const saveMutation = useMutation({
    mutationFn: (input: ResourceInput) => {
      if (!canSubmit)
        throw new Error("You do not have permission to change this resource.");
      return options.id
        ? resource.api.update(options.id, input)
        : resource.api.create(input);
    },
    onSuccess: async (record) => {
      setBaseline(record);
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
    },
  });
  const form = useForm({
    defaultValues,
    validators: { onSubmit: validationSchema },
    onSubmit: async ({ value }) => {
      setServerErrors({});
      setConflict(false);
      saveMutation.reset();
      const entries = fields
        .filter((field) => editing || form.getFieldMeta(field.name)?.isTouched)
        .map((field) => [
          field.name,
          toApiValue(value[field.name], field, baseline?.[field.name]),
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
    form.reset(recordFormValues(record, fields));
    setServerErrors({});
    setConflict(false);
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

function buildValidationSchema(
  fields: FieldDefinition[],
): z.ZodType<FormValues, FormValues> {
  return z
    .object(
      Object.fromEntries(
        fields.map((field) => [field.name, fieldSchema(field)]),
      ),
    )
    .superRefine((values, context) => {
      for (const field of fields) {
        if (field.semantic?.kind !== "money") continue;
        const amount = values[field.name];
        const currency = values[field.semantic.currencyField];
        if (Boolean(amount) === Boolean(currency)) continue;
        context.addIssue({
          code: "custom",
          path: [amount ? field.semantic.currencyField : field.name],
          message: `${field.label} and currency must be provided together`,
        });
      }
    }) as z.ZodType<FormValues, FormValues>;
}
