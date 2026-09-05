import {
  createColumnHelper,
  rowSelectionFeature,
  tableFeatures,
  useTable,
  type RowSelectionState,
} from "@tanstack/react-table";
import { ArrowDown, ArrowUp, Eye, RotateCcw, Trash2 } from "lucide-react";
import { type InputHTMLAttributes, useEffect, useRef } from "react";
import { formatMoney } from "../../field-values";
import { Link } from "../../navigation";
import { RelationValue, useRelationRecords } from "../../relations";
import type {
  AccessActor,
  FieldDefinition,
  ResourceDefinition,
  ResourceRecord,
} from "../../resource";
import { canAccessResource, canAccessRule } from "../../resource";
import { InlineEditor, supportsInlineEdit } from "./InlineEditor";

const resourceTableFeatures = tableFeatures({ rowSelectionFeature });
const resourceColumnHelper = createColumnHelper<
  typeof resourceTableFeatures,
  ResourceRecord
>();

interface ResourceTableProps {
  resources: ResourceDefinition[];
  resource: ResourceDefinition;
  actor: AccessActor | null;
  records: ResourceRecord[];
  visibleFields: FieldDefinition[];
  sort: string;
  trashMode: boolean;
  pending: boolean;
  fetching: boolean;
  rowSelection: RowSelectionState;
  setRowSelection: (
    selection:
      RowSelectionState | ((current: RowSelectionState) => RowSelectionState),
  ) => void;
  changeSort: (field: string) => void;
  updateField: (
    record: ResourceRecord,
    field: FieldDefinition,
    value: unknown,
  ) => Promise<boolean>;
  remove: (id: string) => Promise<void>;
  restore: (id: string) => Promise<void>;
}

export function ResourceTable({
  resources,
  resource,
  actor,
  records,
  visibleFields,
  sort,
  trashMode,
  pending,
  fetching,
  rowSelection,
  setRowSelection,
  changeSort,
  updateField,
  remove,
  restore,
}: ResourceTableProps) {
  const relations = useRelationRecords(resources, records, visibleFields);
  const columns = [
    resourceColumnHelper.display({
      id: "selection",
      header: ({ table }) => (
        <SelectionCheckbox
          aria-label="Select page"
          checked={table.getIsAllPageRowsSelected()}
          indeterminate={
            table.getIsSomePageRowsSelected() &&
            !table.getIsAllPageRowsSelected()
          }
          onChange={table.getToggleAllPageRowsSelectedHandler()}
        />
      ),
      cell: ({ row }) => (
        <input
          type="checkbox"
          checked={row.getIsSelected()}
          onChange={row.getToggleSelectedHandler()}
          aria-label={`Select ${row.id}`}
        />
      ),
    }),
    ...visibleFields.map((field) =>
      resourceColumnHelper.accessor((record) => record[field.name], {
        id: field.name,
        header: () =>
          field.sortable || field.primaryKey ? (
            <button
              className="sort-button"
              onClick={() => changeSort(field.name)}
            >
              {field.label}
              {sort === field.name ? (
                <ArrowUp size={14} />
              ) : sort === `-${field.name}` ? (
                <ArrowDown size={14} />
              ) : null}
            </button>
          ) : (
            field.label
          ),
        cell: (info) => {
          const record = info.row.original;
          if (field.relation)
            return (
              <RelationValue
                field={field}
                value={record[field.name]}
                resources={resources}
                records={relations}
              />
            );
          const editable =
            !trashMode &&
            supportsInlineEdit(field) &&
            canAccessResource(resource, "update", actor, record) &&
            canAccessRule(
              field.writeAccess ?? { mode: "public" },
              actor,
              record,
            );
          return editable ? (
            <InlineEditor
              record={record}
              field={field}
              onSave={(value) => updateField(record, field, value)}
            />
          ) : (
            <span className={field.semantic ? "semantic-value" : undefined}>
              {formatFieldValue(record, field)}
            </span>
          );
        },
      }),
    ),
    resourceColumnHelper.display({
      id: "actions",
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) => {
        const record = row.original;
        const id = String(record[resource.primaryKey]);
        const canRead = canAccessResource(resource, "read", actor, record);
        const canDelete = canAccessResource(resource, "delete", actor, record);
        return (
          <div className="row-actions">
            {canRead && (
              <Link
                className="icon-button"
                to={`/${resource.slug}/${encodeURIComponent(id)}`}
                title="View"
                aria-label="View"
              >
                <Eye size={16} />
              </Link>
            )}
            {trashMode ? (
              <button
                className="icon-button"
                onClick={() => void restore(id)}
                title="Restore"
                aria-label="Restore"
              >
                <RotateCcw size={16} />
              </button>
            ) : (
              canDelete && (
                <button
                  className="icon-button danger"
                  onClick={() => void remove(id)}
                  title={resource.softDelete ? "Move to trash" : "Delete"}
                  aria-label={resource.softDelete ? "Move to trash" : "Delete"}
                >
                  <Trash2 size={16} />
                </button>
              )
            )}
          </div>
        );
      },
    }),
  ];
  const table = useTable({
    features: resourceTableFeatures,
    columns,
    data: records,
    getRowId: (record) => String(record[resource.primaryKey]),
    state: { rowSelection },
    onRowSelectionChange: setRowSelection,
  });

  return (
    <div className="table-frame">
      <table aria-busy={fetching}>
        <thead>
          {table.getHeaderGroups().map((headerGroup) => (
            <tr key={headerGroup.id}>
              {headerGroup.headers.map((header) => (
                <th
                  key={header.id}
                  className={
                    header.column.id === "selection"
                      ? "selection-cell"
                      : undefined
                  }
                >
                  {header.isPlaceholder ? null : (
                    <table.FlexRender header={header} />
                  )}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody>
          {pending && (
            <tr>
              <td colSpan={table.getAllLeafColumns().length} className="empty">
                Loading...
              </td>
            </tr>
          )}
          {!pending && table.getRowModel().rows.length === 0 && (
            <tr>
              <td colSpan={table.getAllLeafColumns().length} className="empty">
                No records
              </td>
            </tr>
          )}
          {!pending &&
            table.getRowModel().rows.map((row) => (
              <tr key={row.id}>
                {row.getAllCells().map((cell) => (
                  <td
                    key={cell.id}
                    className={
                      cell.column.id === "selection"
                        ? "selection-cell"
                        : cell.column.id === "actions"
                          ? "row-actions-cell"
                          : undefined
                    }
                  >
                    <table.FlexRender cell={cell} />
                  </td>
                ))}
              </tr>
            ))}
        </tbody>
      </table>
    </div>
  );
}

function SelectionCheckbox({
  indeterminate,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { indeterminate?: boolean }) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = Boolean(indeterminate);
  }, [indeterminate]);
  return <input ref={ref} type="checkbox" {...props} />;
}

export function formatValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

export function formatFieldValue(
  record: ResourceRecord,
  field: FieldDefinition,
): string {
  const value = record[field.name];
  if (field.semantic?.kind !== "money") return formatValue(value);
  if (value === null || value === undefined || value === "") return "-";
  const currency = record[field.semantic.currencyField];
  if (typeof currency !== "string") {
    return formatValue(value);
  }
  try {
    return formatMoney(value, currency, field.semantic.fractionDigits);
  } catch {
    return `${currency} ${formatValue(value)}`;
  }
}
