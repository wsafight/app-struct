import type { ResourceRecord } from "../../resource";

/**
 * Map record ids to their current revisions for optimistic-concurrency requests.
 *
 * Records are indexed once instead of scanning the list per id, which matters for bulk
 * operations over large selections.
 */
export function revisionMap(
  records: ResourceRecord[],
  primaryKey: string,
  ids: string[],
): Record<string, number> {
  const byId = new Map(
    records.map((record) => [
      String(record[primaryKey]),
      Number(record.revision ?? 0),
    ]),
  );
  return Object.fromEntries(ids.map((id) => [id, byId.get(id) ?? 0]));
}
