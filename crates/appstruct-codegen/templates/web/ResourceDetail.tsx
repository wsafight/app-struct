import { ArrowLeft, Edit3 } from "lucide-react";
import { useResourceDetailController } from "../controller";
import { Link, useParams } from "../navigation";
import type { ResourceDefinition } from "../resource";
import { canAccessRule, errorMessage, useResourceActor } from "../resource";
import { formatValue } from "./ResourceList";

export function ResourceDetail({ resource }: { resource: ResourceDefinition }) {
  const { id } = useParams();
  const actor = useResourceActor();
  const controller = useResourceDetailController(resource, id);
  const record = controller.record;

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
          <h1>{record ? formatValue(record[resource.primaryKey]) : "Loading..."}</h1>
        </div>
        {id && controller.canUpdate && (
          <Link className="primary-button" to={`/${resource.slug}/${encodeURIComponent(id)}/edit`}>
            <Edit3 size={16} /> Edit
          </Link>
        )}
      </div>
      {controller.error && (
        <div className="alert" role="alert">
          {errorMessage(controller.error)}
        </div>
      )}
      {record && (
        <dl className="detail-grid">
          {resource.fields
            .filter((field) => canAccessRule(field.readAccess ?? { mode: "public" }, actor))
            .map((field) => (
              <div key={field.name}>
                <dt>{field.label}</dt>
                <dd>{formatValue(record[field.name])}</dd>
              </div>
            ))}
        </dl>
      )}
    </main>
  );
}
