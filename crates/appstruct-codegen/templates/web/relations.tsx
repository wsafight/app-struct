import { useQueries } from "@tanstack/react-query";
import { Link } from "./navigation";
import { resourceQueryKeys } from "./query";
import {
  canAccessResource,
  canAccessRule,
  useResourceActor,
  type FieldDefinition,
  type ResourceDefinition,
  type ResourceRecord,
} from "./resource";

export function recordLabel(
  resource: ResourceDefinition,
  record: ResourceRecord,
): string {
  const field =
    resource.displayField ??
    resource.fields.find(
      (field) => !field.primaryKey && ["string", "text"].includes(field.kind),
    )?.name;
  const label = field ? record[field] : undefined;
  return String(
    label == null || label === ""
      ? (record[resource.primaryKey] ?? "-")
      : label,
  );
}

export function useRelationRecords(
  resources: ResourceDefinition[],
  records: ResourceRecord[],
  fields: FieldDefinition[],
) {
  const actor = useResourceActor();
  const requests = resources.flatMap((target) => {
    if (!target.api.lookup || !canAccessResource(target, "read", actor))
      return [];
    const related = fields.filter(
      (field) =>
        field.relation === target.id &&
        canAccessRule(field.readAccess ?? { mode: "public" }, actor),
    );
    const ids = [
      ...new Set(
        records.flatMap((record) =>
          related
            .map((field) => record[field.name])
            .filter((value) => value != null && value !== "")
            .map(String),
        ),
      ),
    ].sort();
    const batches = [];
    for (let start = 0; start < ids.length; start += 100)
      batches.push({ target, ids: ids.slice(start, start + 100) });
    return batches;
  });
  const queries = useQueries({
    queries: requests.map(({ target, ids }) => ({
      queryKey: [...resourceQueryKeys.all(target.id), "lookup", ids],
      queryFn: ({ signal }: { signal: AbortSignal }) =>
        target.api.lookup!(ids, { signal }),
      staleTime: 30_000,
    })),
  });
  const result = new Map<string, ResourceRecord>();
  requests.forEach(({ target }, index) => {
    for (const record of queries[index].data ?? [])
      result.set(`${target.id}:${record[target.primaryKey]}`, record);
  });
  return result;
}

export function RelationValue({
  field,
  value,
  resources,
  records,
}: {
  field: FieldDefinition;
  value: unknown;
  resources: ResourceDefinition[];
  records: Map<string, ResourceRecord>;
}) {
  const target = resources.find((resource) => resource.id === field.relation);
  const record = target ? records.get(`${target.id}:${value}`) : undefined;
  if (!target || !record)
    return <span>{value == null || value === "" ? "-" : String(value)}</span>;
  return (
    <Link
      to={`/${target.slug}/${encodeURIComponent(String(record[target.primaryKey]))}`}
    >
      {recordLabel(target, record)}
    </Link>
  );
}
