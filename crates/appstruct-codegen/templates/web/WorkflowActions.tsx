import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Play } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import { DialogFrame } from "../components/Dialog";
import { inputType, toApiValue } from "../field-values";
import { resourceQueryKeys } from "../query";
import {
  errorMessage,
  type ResourceDefinition,
  type WorkflowInputFieldDefinition,
  type WorkflowTransitionDefinition,
} from "../resource";

type InputValue = string | boolean;

export function WorkflowActions({
  resource,
  id,
}: {
  resource: ResourceDefinition;
  id: string;
}) {
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<WorkflowTransitionDefinition | null>(
    null,
  );
  const [values, setValues] = useState<Record<string, InputValue>>({});
  const [inputError, setInputError] = useState<string | null>(null);
  const workflow = resource.workflow;
  const capabilityKey = ["resource", resource.id, id, "workflow"] as const;
  const capabilityQuery = useQuery({
    queryKey: capabilityKey,
    queryFn: ({ signal }) => resource.api.transitions!(id, { signal }),
    enabled: Boolean(workflow && resource.api.transitions),
  });
  const definitions = useMemo(() => {
    const allowed = new Set(
      capabilityQuery.data?.allowed_transitions.map((item) => item.name) ?? [],
    );
    return workflow?.transitions.filter((item) => allowed.has(item.name)) ?? [];
  }, [capabilityQuery.data, workflow]);
  const mutation = useMutation({
    mutationFn: ({
      transition,
      input,
    }: {
      transition: WorkflowTransitionDefinition;
      input: unknown;
    }) => resource.api.transition!(id, transition.name, input),
    onSuccess: async (next) => {
      setSelected(null);
      queryClient.setQueryData(resourceQueryKeys.detail(resource.id, id), next);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: capabilityKey }),
        queryClient.invalidateQueries({
          queryKey: resourceQueryKeys.all(resource.id),
        }),
      ]);
    },
    onError: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: capabilityKey }),
        queryClient.invalidateQueries({
          queryKey: resourceQueryKeys.detail(resource.id, id),
        }),
      ]);
    },
  });

  if (!workflow || !resource.api.transitions || !resource.api.transition) {
    return null;
  }

  function begin(transition: WorkflowTransitionDefinition) {
    if (!transition.input) {
      mutation.mutate({ transition, input: {} });
      return;
    }
    setValues(
      Object.fromEntries(
        transition.input.fields.map((field) => [
          field.name,
          field.kind === "boolean" ? false : "",
        ]),
      ),
    );
    setInputError(null);
    setSelected(transition);
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!selected?.input) return;
    try {
      mutation.mutate({
        transition: selected,
        input: workflowInput(selected.input.fields, values),
      });
    } catch (error) {
      setInputError(errorMessage(error));
    }
  }

  return (
    <>
      <div className="workflow-actions" aria-label="Workflow actions">
        {definitions.map((transition) => (
          <button
            key={transition.name}
            type="button"
            className="secondary-button"
            disabled={mutation.isPending}
            onClick={() => begin(transition)}
          >
            <Play size={15} /> {transition.label}
          </button>
        ))}
      </div>
      {(capabilityQuery.error || mutation.error) && (
        <div className="workflow-error" role="alert">
          {errorMessage(mutation.error ?? capabilityQuery.error)}
        </div>
      )}
      <DialogFrame
        open={selected !== null}
        title={selected?.label ?? "Workflow action"}
        onCancel={() => setSelected(null)}
      >
        <form className="dialog-form workflow-input" onSubmit={submit}>
          {selected?.input?.fields.map((field) => (
            <WorkflowField
              key={field.name}
              field={field}
              value={values[field.name]}
              onChange={(value) =>
                setValues((current) => ({ ...current, [field.name]: value }))
              }
            />
          ))}
          {inputError && (
            <div className="alert" role="alert">
              {inputError}
            </div>
          )}
          <div className="dialog-actions">
            <button
              type="button"
              className="secondary-button"
              disabled={mutation.isPending}
              onClick={() => setSelected(null)}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="primary-button"
              disabled={mutation.isPending}
            >
              <Play size={15} />
              {mutation.isPending ? "Working..." : selected?.label}
            </button>
          </div>
        </form>
      </DialogFrame>
    </>
  );
}

function WorkflowField({
  field,
  value,
  onChange,
}: {
  field: WorkflowInputFieldDefinition;
  value: InputValue | undefined;
  onChange(value: InputValue): void;
}) {
  if (field.kind === "boolean") {
    return (
      <label className="workflow-checkbox">
        <input
          type="checkbox"
          checked={Boolean(value)}
          onChange={(event) => onChange(event.target.checked)}
        />
        {field.label}
      </label>
    );
  }
  if (field.kind === "enum") {
    return (
      <label>
        {field.label}
        <select
          required={field.required}
          value={String(value ?? "")}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">Select...</option>
          {field.values?.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      </label>
    );
  }
  if (field.kind === "text" || field.kind === "json") {
    return (
      <label>
        {field.label}
        <textarea
          required={field.required}
          value={String(value ?? "")}
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
    );
  }
  return (
    <label>
      {field.label}
      <input
        required={field.required}
        type={inputType(field.kind)}
        inputMode={
          field.kind === "bigint"
            ? "numeric"
            : field.kind === "decimal"
              ? "decimal"
              : undefined
        }
        step={field.kind === "datetime" ? "any" : undefined}
        value={String(value ?? "")}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

function workflowInput(
  fields: WorkflowInputFieldDefinition[],
  values: Record<string, InputValue>,
): Record<string, unknown> {
  return Object.fromEntries(
    fields.flatMap((field) => {
      const value = values[field.name];
      if (value === "" && !field.required) return [];
      if (value === "" && field.required) {
        throw new Error(`${field.label} is required`);
      }
      return [[field.name, toApiValue(value, field)]];
    }),
  );
}
