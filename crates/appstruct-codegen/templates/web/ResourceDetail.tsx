import { t } from "../ui";
import { ArrowLeft, Download, Edit3 } from "lucide-react";
import { AsyncState } from "../components/AsyncState";
import { useResourceDetailController } from "../controller";
import { fileFieldApi } from "../generated/client";
import { Link, useParams } from "../navigation";
import { RelationValue, recordLabel, useRelationRecords } from "../relations";
import type { ResourceDefinition } from "../resource";
import type { AppStructRegistry } from "../generated/registry";
import { AggregateEditor } from "./AggregateEditor";
import {
  canAccessRule,
  errorMessage,
  isSemanticCompanion,
  triggerDownload,
  useResourceActor,
} from "../resource";
import { formatFieldValue } from "./ResourceList";
import { WorkflowActions } from "./WorkflowActions";
__DETAIL_IMPORTS__;

export function ResourceDetail({
  resource,
  resources,
  registry,
}: {
  resource: ResourceDefinition;
  resources: ResourceDefinition[];
  registry?: AppStructRegistry;
}) {
  const { id } = useParams();
  const actor = useResourceActor();
  const controller = useResourceDetailController(resource, id);
  const record = controller.record;
  const relations = useRelationRecords(
    resources,
    record ? [record] : [],
    resource.fields,
  );

  if (!controller.canRead) {
    return (
      <main className="page">
        <div className="alert" role="alert">
          {t("You do not have permission to view this resource.")}{" "}
        </div>
      </main>
    );
  }

  return (
    <main className="page detail-page">
      <div className="page-heading">
        <div>
          <Link className="back-link" to={`/${resource.slug}`}>
            <ArrowLeft size={16} /> {resource.label}
          </Link>
          <h1>{record ? recordLabel(resource, record) : resource.label}</h1>
        </div>
        <div className="detail-actions">
          {id && record && <WorkflowActions id={id} resource={resource} />}
          {id && controller.canUpdate && (
            <Link
              className="primary-button"
              to={`/${resource.slug}/${encodeURIComponent(id)}/edit`}
            >
              <Edit3 size={16} /> {t("Edit")}
            </Link>
          )}
        </div>
      </div>
      {controller.pending && (
        <AsyncState state="loading" message="Loading record" />
      )}
      {!controller.pending && controller.error && (
        <AsyncState
          state="error"
          message={errorMessage(controller.error)}
          onRetry={() => void controller.refetch()}
        />
      )}
      {!controller.pending && !controller.error && !record && (
        <AsyncState state="empty" message="Record not found" />
      )}
      {record && (
        <>
          <dl className="detail-grid">
            {resource.fields
              .filter(
                (field) =>
                  !isSemanticCompanion(field, resource.fields) &&
                  canAccessRule(field.readAccess ?? { mode: "public" }, actor),
              )
              .map((field) => (
                <div key={field.name}>
                  <dt>{field.label}</dt>
                  <dd className={field.semantic ? "semantic-value" : undefined}>
                    {field.relation ? (
                      <RelationValue
                        field={field}
                        value={record[field.name]}
                        resources={resources}
                        records={relations}
                      />
                    ) : field.semantic?.kind === "file" &&
                      record[field.name] ? (
                      <span className="row-actions">
                        <code>{String(record[field.name])}</code>
                        <button
                          type="button"
                          className="secondary-button"
                          onClick={() => {
                            const fileId = String(record[field.name]);
                            void fileFieldApi
                              .download(fileId)
                              .then((blob) =>
                                triggerDownload(blob, `${field.name}-${fileId}`),
                              );
                          }}
                        >
                          <Download size={14} /> {t("Download")}
                        </button>
                      </span>
                    ) : (
                      formatFieldValue(record, field)
                    )}
                  </dd>
                </div>
              ))}
          </dl>
          {id &&
            resource.collections?.map((definition) => {
              const child = resources.find(
                (item) => item.id === definition.child,
              );
              return child ? (
                <AggregateEditor
                  key={`${id}:${definition.name}`}
                  parent={resource}
                  child={child}
                  definition={definition}
                  id={id}
                  resources={resources}
                  registry={registry}
                />
              ) : null;
            })}
          __DETAIL_EXTRAS__
        </>
      )}
    </main>
  );
}
