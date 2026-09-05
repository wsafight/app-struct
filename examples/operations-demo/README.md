# operations-demo

This canonical example combines the existing AppStruct operational contracts in one bounded order
workflow. It is an application example, not a CLI template or an ERP starter.

The domain contains products, supplier offers, inventory records, orders, and standalone order
lines. An operator creates an order and its lines, submits it, and an auditor approves or rejects
it. Reports produce tenant-bound PDF files, and the order detail timeline records comments, CRUD
events, and workflow transitions.

1. Copy `.env.example` to `.env` and replace the development-only report snapshot key.
2. Ensure Docker with Compose is running.
3. Run `appstruct doctor`.
4. Run `appstruct dev`.

The API listens on `http://127.0.0.1:3000` and the Web application on
`http://127.0.0.1:5173` by default. New accounts receive the `operator` role. Role assignment is an
explicit application operation; repository E2E tests seed their role-specific accounts directly in
their isolated database.

`OrderLine` deliberately remains an ordinary related entity. The example does not promise nested
line-item editing, inventory reservation, accounting, payment processing, or cross-aggregate sagas.
Money and quantity fields use the existing decimal, integer, enum, and relation contracts so the
example can expose repeated UI friction before any semantic UI contract is added.
