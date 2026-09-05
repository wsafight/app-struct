# Scalar Values

Business `bigint` fields use decimal strings in JSON, OpenAPI and TypeScript. PostgreSQL columns
and Rust values remain signed 64-bit integers. This includes entity fields, value objects,
Workflow inputs and bigint aggregate/group values. Decimal values and aggregates are also strings.
Framework revision counters and pagination metadata retain their existing numeric contracts.

Generated backends accept legacy JSON integer inputs only within JavaScript's safe range,
`-9007199254740991` through `9007199254740991`. Larger values must be strings. Responses always use
strings. Regenerate and deploy clients with the corresponding backend when adopting this preview
contract; clients that perform arithmetic on bigint fields must explicitly use BigInt or a decimal
library. Existing data and migration files do not change.

The shared Web field-value API preserves these strings across forms, inline editing and Workflow
dialogs. Numeric bounds use exact decimal comparisons. Monetary formatting never converts amounts
to a JavaScript Number. JSON fields remain arbitrary JSON and do not infer types for nested values.

Datetime API values identify UTC instants. Generated controls display browser-local calendar time,
including seconds and up to PostgreSQL's six fractional digits, and convert edited values back to
UTC. Unchanged values retain their original instant, including the later occurrence of a repeated
daylight-saving hour. Newly entered ambiguous local times use the browser's earlier occurrence;
invalid calendar dates and times in a daylight-saving gap are rejected. Date-only fields have no
timezone conversion.
