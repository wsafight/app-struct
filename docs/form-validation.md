# Common form rules

Fields support portable, same-record rules in addition to scalar bounds. The compiler validates references and types; generated Web forms use `valueError` and `recordRuleErrors`, without a direct Zod dependency.

```yaml
fields:
  starts: { type: datetime, required: true }
  ends: { type: datetime, greater_than: starts }
  needs_note: { type: boolean, required: true }
  note:
    type: text
    required_when: { field: needs_note, equals: true }
```

`greater_than` is strict: equal values fail. Both fields must have the same integer, bigint, decimal, date or datetime type. Comparison runs only when both are present. Decimal and microsecond datetime comparisons preserve precision. `required_when` accepts a sibling boolean/text/enum field and a matching boolean/string `equals`; its target must be a nullable scalar. Missing/null/empty text fails when the condition matches; zero and false are present values. Clearing the condition does not automatically clear existing target data.

Rule targets must be ordinary fields, and dependencies must be different sibling fields. Both require unrestricted field read/write access to avoid exposing hidden state through validation. Conditions may depend on a Workflow state. Use Policy/Hooks for rules involving restricted fields, other records or custom business logic; arbitrary expressions and SQL are not accepted.

The backend validates the complete candidate after input Hooks and permission checks, before writing. Create, partial update, CSV, bulk update, aggregate collection writes and Workflow transitions share this check. A partial patch cannot bypass a rule by omitting its other field. Invalid writes return 422 with field violations; bulk/CSV report failures per row. Transaction rollback prevents Hook side effects from committing with a rejected candidate. Raw SQL, seeds and application code writing entity models directly remain application responsibilities; these rules do not create SQL CHECK constraints.

The Web validates known editable values and unchanged baseline values; server defaults and runtime state are authoritative. Bulk updates and inline controls can receive server field errors because they may not load a complete record. Locale settings affect messages and display, not rule semantics. Recompile the current IR (18); older IR is rejected.
