import type {
  AppStructRegistry,
  FieldComponentProps,
  PageComponentProps,
} from "../../generated/web/src/generated/registry";

function ProjectMetadataEditor({
  label,
  value,
  error,
  readOnly,
  onChange,
}: FieldComponentProps) {
  return (
    <>
      <textarea
        aria-label={label}
        value={String(value ?? "")}
        readOnly={readOnly}
        onChange={(event) => onChange(event.target.value)}
      />
      {error && <small role="alert">{error}</small>}
    </>
  );
}

function ProjectDashboard({ resources }: PageComponentProps) {
  return (
    <main className="page">
      <div className="page-heading">
        <div>
          <h1>Project dashboard</h1>
          <p>{resources.length} resources</p>
        </div>
      </div>
    </main>
  );
}

export const registry = {
  fields: { ProjectMetadataEditor },
  pages: { ProjectDashboard },
} satisfies AppStructRegistry;
