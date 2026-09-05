import { ArrowLeft, Edit3 } from "lucide-react";
import { useResourceDetailController } from "../controller";
import { Link, useParams } from "../navigation";
import { RelationValue, recordLabel, useRelationRecords } from "../relations";
import type { ResourceDefinition } from "../resource";
import type { AppStructRegistry } from "../generated/registry";
import { AggregateEditor } from "./AggregateEditor";
import {
  canAccessRule,
  errorMessage,
  isSemanticCompanion,
  useResourceActor,
} from "../resource";
import { formatFieldValue } from "./ResourceList";
import { WorkflowActions } from "./WorkflowActions";__DETAIL_IMPORTS__

export function ResourceDetail({ resource, resources, registry }: { resource: ResourceDefinition; resources: ResourceDefinition[]; registry?: AppStructRegistry }) {
  const { id } = useParams();
  const actor = useResourceActor();
  const controller = useResourceDetailController(resource, id);
  const record = controller.record;
  const relations = useRelationRecords(resources, record ? [record] : [], resource.fields);

  if (!controller.canRead) {
    return (
      <main className="page">
        <div className="alert" role="alert">
          You do not have permission to view this resource.
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
          <h1>
            {record ? recordLabel(resource, record) : "Loading..."}
          </h1>
        </div>
        <div className="detail-actions">
          {id && record && <WorkflowActions id={id} resource={resource} />}
          {id && controller.canUpdate && (
            <Link
              className="primary-button"
              to={`/${resource.slug}/${encodeURIComponent(id)}/edit`}
            >
              <Edit3 size={16} /> Edit
            </Link>
          )}
        </div>
      </div>
      {controller.error && (
        <div className="alert" role="alert">
          {errorMessage(controller.error)}
        </div>
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
                    {field.relation ? <RelationValue field={field} value={record[field.name]} resources={resources} records={relations} /> : formatFieldValue(record, field)}
                  </dd>
                </div>
              ))}
          </dl>
          {id && resource.collections?.map((definition) => {
            const child = resources.find((item) => item.id === definition.child);
            return child ? <AggregateEditor key={`${id}:${definition.name}`} parent={resource} child={child} definition={definition} id={id} resources={resources} registry={registry} /> : null;
          })}__DETAIL_EXTRAS__
        </>
      )}
    </main>
  );
}
