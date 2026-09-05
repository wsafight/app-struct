# Entity Workflows

Workflow v1 turns one required enum field into a server-managed state machine. The compiler removes
that field from normal create, update, bulk update, and CSV import inputs; create writes the declared
initial state and later changes go through transition endpoints.

```yaml
entities:
  Order:
    fields:
      status:
        type: enum
        required: true
        values: [draft, auditing, approved, rejected]
    workflow:
      field: status
      initial: draft
      transitions:
        submit:
          from: [draft, rejected]
          to: auditing
          access: { owner: owner }
        approve:
          from: [auditing]
          to: approved
          access: { role: auditor }
        reject:
          from: [auditing]
          to: rejected
          input: RejectOrderInput
          access: { role: auditor }
```

The workflow field must be a required enum without `default`, `generated`, or field-level write
access. Every source, target, and initial state must be an enum value. Transition names and edges
must be unique, referenced input value objects must exist, and every enum state must be reachable
from the initial state.

## Generated Contract

For an entity whose table is `orders`, the generated backend exposes:

- `GET /api/orders/{id}/_transitions` to return the current state, revision, and transitions the
  current actor may see.
- `POST /api/orders/{id}/_transitions/{action}` to execute one transition. The request must include
  the latest ETag in `If-Match`; the JSON body is the configured value object or `{}` for a
  transition without input.

The generated TypeScript resource client exposes `transitions(id)` and
`transition(id, action, input?)`. The React detail view fetches capabilities and only renders actions
returned by the backend. This is a convenience, not an authorization boundary.

Execution applies tenant and read scopes, loads the row with `FOR UPDATE`, checks the revision and
source state, evaluates declarative access, runs `before_transition`, checks the extension Policy,
updates state and revision, and runs `after_transition`. The write and all enabled integrations are
committed in one transaction. An audited entity records `workflow.<action>` with an input digest;
Activity records the same system event. Webhooks use `<entity_event_prefix>.workflow.<action>`, and
Realtime publishes that event after commit.

Stable workflow errors include `UNKNOWN_WORKFLOW_TRANSITION` (404), `INVALID_WORKFLOW_STATE` (409),
`INVALID_WORKFLOW_INPUT` (422), `PRECONDITION_REQUIRED` (428), and
`CONCURRENT_MODIFICATION` (412). Invisible records return the normal not-found response.

## V1 Boundaries

Workflow v1 supports one managed workflow field per entity. It does not implement BPMN, arbitrary
scripts, request idempotency keys, sagas, or distributed multi-aggregate workflows. A repeated
submission is resolved by the current-state and revision checks. Use ordinary Commands for actions
that are not state transitions.
