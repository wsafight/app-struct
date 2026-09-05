import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Edit3, Plus, RefreshCw, Save, Trash2, X } from "lucide-react";
import { useState } from "react";
import {
  collectionBatch,
  collectionRow,
  type CollectionRow,
} from "../collection-draft";
import type { FormValue } from "../field-values";
import type { AppStructRegistry } from "../generated/registry";
import type { CollectionResponse } from "../generated/client";
import { resourceQueryKeys } from "../query";
import { RelationValue, useRelationRecords } from "../relations";
import {
  canAccessRule,
  canAccessResource,
  errorMessage,
  fieldErrors,
  isSemanticCompanion,
  useResourceActor,
  type CollectionDefinition,
  type ResourceDefinition,
} from "../resource";
import { FieldControl, UnsavedChangesGuard } from "./ResourceForm";
import { formatFieldValue } from "./ResourceList";

export function AggregateEditor({
  parent,
  child,
  definition,
  id,
  resources,
  registry,
}: {
  parent: ResourceDefinition;
  child: ResourceDefinition;
  definition: CollectionDefinition;
  id: string;
  resources: ResourceDefinition[];
  registry?: AppStructRegistry;
}) {
  const queryClient = useQueryClient();
  const actor = useResourceActor();
  const queryKey = [
    ...resourceQueryKeys.detail(parent.id, id),
    "collection",
    definition.name,
  ];
  const query = useQuery({
    queryKey,
    queryFn: ({ signal }) =>
      parent.api.collection!(id, definition.name, { signal }),
  });
  const [baseline, setBaseline] = useState<CollectionResponse>();
  const [rows, setRows] = useState<CollectionRow[]>([]);
  const [editing, setEditing] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const data = baseline && editing ? baseline : query.data;
  const fields = child.fields.filter(
    (field) =>
      !field.readOnly &&
      !field.primaryKey &&
      field.name !== definition.relation &&
      canAccessRule(field.readAccess ?? { mode: "public" }, actor) &&
      canAccessRule(field.writeAccess ?? { mode: "public" }, actor),
  );
  const visible = child.fields.filter(
    (field) =>
      !field.primaryKey &&
      field.name !== definition.relation &&
      !isSemanticCompanion(field, child.fields) &&
      canAccessRule(field.readAccess ?? { mode: "public" }, actor),
  );
  const relations = useRelationRecords(resources, data?.rows ?? [], visible);
  const draft = collectionBatch(rows, baseline?.rows ?? [], child, fields);
  const allowed =
    data &&
    canAccessResource(parent, "update", actor, data.parent) &&
    (!parent.workflow ||
      definition.states.includes(String(data.parent[parent.workflow.field])));
  const save = useMutation({
    mutationFn: () =>
      parent.api.saveCollection!(
        id,
        definition.name,
        Number(baseline!.parent.revision),
        draft.batch,
      ),
    onSuccess: async (result) => {
      setEditing(false);
      setBaseline(undefined);
      setErrors({});
      queryClient.setQueryData(queryKey, result);
      queryClient.setQueryData(
        resourceQueryKeys.detail(parent.id, id),
        result.parent,
      );
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: resourceQueryKeys.all(parent.id),
        }),
        queryClient.invalidateQueries({
          queryKey: resourceQueryKeys.all(child.id),
        }),
      ]);
    },
    onError: (error) =>
      setErrors(
        Object.fromEntries(
          Object.entries(fieldErrors(error)).map(([key, value]) => [
            key.replace(/^(creates|updates)\./, ""),
            value,
          ]),
        ),
      ),
  });
  function begin(result: CollectionResponse) {
    setBaseline(result);
    setRows(result.rows.map((record) => collectionRow(fields, child, record)));
    setErrors({});
    save.reset();
    setEditing(true);
  }
  function change(key: string, name: string, value: FormValue) {
    setRows((current) =>
      current.map((row) =>
        row.key === key
          ? { ...row, values: { ...row.values, [name]: value } }
          : row,
      ),
    );
    setErrors((current) => {
      const next = { ...current };
      delete next[`${key}.${name}`];
      return next;
    });
  }
  async function reload() {
    const result = await query.refetch();
    if (result.data && !result.error) begin(result.data);
  }
  return (
    <section className="aggregate-section" aria-label={child.label}>
      <UnsavedChangesGuard
        enabled={editing && draft.dirty && !save.isPending}
      />
      <div className="section-heading">
        <h2>{child.label}</h2>
        {!editing && allowed && (
          <button className="secondary-button" onClick={() => begin(data!)}>
            <Edit3 size={16} /> Edit lines
          </button>
        )}
      </div>
      {(query.error || save.error) && (
        <div className="alert" role="alert">
          {errorMessage(save.error ?? query.error)}
        </div>
      )}
      {query.isPending && <p role="status">Loading...</p>}
      {editing ? (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            setErrors(draft.errors);
            if (!Object.keys(draft.errors).length && draft.dirty) save.mutate();
          }}
        >
          <fieldset className="aggregate-fields" disabled={save.isPending}>
            {rows.map((row, index) => {
              const canUpdate =
                !row.record ||
                canAccessRule(child.access.update, actor, row.record);
              return (
                <fieldset className="aggregate-row" key={row.key}>
                  <legend>Line {index + 1}</legend>
                  <fieldset
                    className="aggregate-row-fields"
                    disabled={!canUpdate}
                  >
                    {fields
                      .filter((field) => !isSemanticCompanion(field, fields))
                      .map((field) => {
                        const companion =
                          field.semantic?.kind === "money"
                            ? fields.find(
                                (candidate) =>
                                  candidate.name ===
                                  field.semantic!.currencyField,
                              )
                            : undefined;
                        return (
                          <FieldControl
                            key={field.name}
                            idPrefix={`${definition.name}-${row.key}-`}
                            field={field}
                            resources={resources}
                            registry={registry}
                            value={row.values[field.name]}
                            error={errors[`${row.key}.${field.name}`]}
                            onBlur={() => {}}
                            onChange={(value) =>
                              change(row.key, field.name, value)
                            }
                            companion={
                              companion
                                ? {
                                    field: companion,
                                    value: row.values[companion.name],
                                    error:
                                      errors[`${row.key}.${companion.name}`],
                                    onBlur() {},
                                    onChange: (value) =>
                                      change(row.key, companion.name, value),
                                  }
                                : undefined
                            }
                          />
                        );
                      })}
                  </fieldset>
                  {(!row.record ||
                    canAccessRule(child.access.delete, actor, row.record)) && (
                    <button
                      type="button"
                      className="icon-button danger"
                      aria-label={`Remove line ${index + 1}`}
                      title={`Remove line ${index + 1}`}
                      onClick={() =>
                        setRows((current) =>
                          current.filter((item) => item.key !== row.key),
                        )
                      }
                    >
                      <Trash2 size={16} />
                    </button>
                  )}
                </fieldset>
              );
            })}
            <div className="aggregate-actions">
              {canAccessRule(child.access.create, actor) && (
                <button
                  type="button"
                  className="secondary-button"
                  disabled={rows.length >= definition.maxItems}
                  onClick={() =>
                    setRows((current) => [
                      ...current,
                      collectionRow(fields, child),
                    ])
                  }
                >
                  <Plus size={16} /> Add line
                </button>
              )}
              <button
                type="submit"
                className="primary-button"
                disabled={!draft.dirty}
              >
                <Save size={16} /> {save.isPending ? "Saving..." : "Save lines"}
              </button>
              <button
                type="button"
                className="secondary-button"
                disabled={query.isFetching}
                onClick={() => void reload()}
              >
                <RefreshCw size={16} /> Reload
              </button>
              <button
                type="button"
                className="secondary-button"
                onClick={() => {
                  setEditing(false);
                  save.reset();
                }}
              >
                <X size={16} /> Cancel
              </button>
            </div>
          </fieldset>
        </form>
      ) : (
        data && (
          <div className="table-frame">
            <table>
              <thead>
                <tr>
                  {visible.map((field) => (
                    <th key={field.name}>{field.label}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {data.rows.map((record) => (
                  <tr key={String(record[child.primaryKey])}>
                    {visible.map((field) => (
                      <td key={field.name}>
                        {field.relation ? (
                          <RelationValue
                            field={field}
                            value={record[field.name]}
                            resources={resources}
                            records={relations}
                          />
                        ) : (
                          formatFieldValue(record, field)
                        )}
                      </td>
                    ))}
                  </tr>
                ))}
                {!data.rows.length && (
                  <tr>
                    <td colSpan={visible.length}>No lines</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )
      )}
    </section>
  );
}
