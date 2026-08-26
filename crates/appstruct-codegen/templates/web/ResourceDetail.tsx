import { ArrowLeft, Edit3 } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import type { ResourceDefinition, ResourceRecord } from "../resource";
import { errorMessage, useCanAccess } from "../resource";
import { formatValue } from "./ResourceList";

export function ResourceDetail({ resource }: { resource: ResourceDefinition }) {
  const { id } = useParams();
  const [record, setRecord] = useState<ResourceRecord>();
  const [error, setError] = useState("");
  const canRead = useCanAccess(resource, "read");
  const canUpdate = useCanAccess(resource, "update", record);
  useEffect(() => {
    if (!id || !canRead) return;
    resource.api.get(id).then(setRecord).catch((reason) => setError(errorMessage(reason)));
  }, [canRead, id, resource]);
  if (!canRead) return <main className="page"><div className="alert" role="alert">You do not have permission to view this resource.</div></main>;
  return <main className="page detail-page">
    <div className="page-heading"><div><Link className="back-link" to={`/${resource.slug}`}><ArrowLeft size={16} /> {resource.label}</Link><h1>{record ? formatValue(record[resource.primaryKey]) : "Loading..."}</h1></div>{id && canUpdate && <Link className="primary-button" to={`/${resource.slug}/${encodeURIComponent(id)}/edit`}><Edit3 size={16} /> Edit</Link>}</div>
    {error && <div className="alert" role="alert">{error}</div>}
    {record && <dl className="detail-grid">{resource.fields.map((field) => <div key={field.name}><dt>{field.label}</dt><dd>{formatValue(record[field.name])}</dd></div>)}</dl>}
  </main>;
}
