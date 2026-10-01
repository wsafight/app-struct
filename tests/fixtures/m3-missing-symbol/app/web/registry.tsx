import type {
  AppStructRegistry,
  FieldComponentProps,
  PageComponentProps,
} from "../../generated/web/src/generated/registry";

function ProjectMetadataEditor({
  id,
  label,
  value,
  error,
  readOnly,
  onChange,
}: FieldComponentProps) {
  return (
    <>
      <textarea
        id={id}
        aria-label={label}
        value={String(value ?? "")}
        readOnly={readOnly}
        onChange={(event) => onChange(event.target.value)}
      />
      {error && <small role="alert">{error}</small>}
    </>
  );
}

export const registry = {
  fields: { ProjectMetadataEditor },
  pages: {},
} satisfies AppStructRegistry;
