# Business UI Semantics

> Status: money v1 accepted; quantity deferred
> Date: 2026-09-05

## Purpose

Business UI semantics describe how existing scalar fields are presented and edited. They do not
add database types, change REST payloads, or weaken field and resource authorization. The first
contract is intentionally limited to a monetary amount and a currency field on the same entity.

The Operations Demo contains this shape in both `SupplierOffer` and `OrderLine`, which is enough to
justify one reusable presentation contract. Its quantity fields derive their units through a
Product relation, so quantity remains deferred until relation display data has a defined loading
and consistency contract.

## Money v1

```yaml
unit_price:
  type: decimal
  required: true
  ui:
    semantic: money
    currency_field: currency
    fraction_digits: 2
currency:
  type: enum
  required: true
  values: [CNY, USD, EUR]
```

`ui.semantic: money` is valid only when all of these invariants hold:

- the annotated amount is a non-generated Decimal field;
- `currency_field` names a sibling Enum field;
- every currency value is a three-letter uppercase ISO-style code;
- amount and currency are either both required or both optional;
- amount and currency have identical field-level read and write access;
- the currency field is not a custom component, another semantic field, or the companion of a
  second money field;
- `fraction_digits` is an integer from 0 through 6;
- `ui.component` and `ui.semantic` are mutually exclusive.

The generated Web runtime renders one combined amount/currency control. List and detail values use
the companion currency code, fixed fraction digits, locale-aware separators, and tabular numeric
alignment. The companion currency remains available for filtering and explicit column selection,
but is omitted from the default columns and from the detail grid because the money value already
contains it. Semantic fields are not inline editable in v1 because amount and currency must be
saved together.

`fraction_digits` controls Web input stepping and display rounding only. It is not a storage scale
or an API validation rule. Applications that require an exact accounting scale must enforce it in
their domain validation. When omitted it defaults to 2.

If a custom client receives metadata it cannot satisfy, it must fall back to the underlying Decimal
and Enum fields. The API continues to authorize and validate both fields independently.

## Deferred Quantity Contract

The Operations Demo stores Product unit separately from Inventory and OrderLine quantity. A useful
quantity control would therefore need relation label expansion, loading and unavailable states,
authorization for the related Product, and a rule for unit changes after records already exist.
Those contracts are not part of money v1. A same-record `quantity + unit` syntax should not be added
until at least one real example needs that data shape.

## Non-goals

- currency conversion, exchange rates, taxes, totals, or accounting rules;
- a PostgreSQL money type or a new REST scalar;
- locale or currency inference from the tenant;
- semantic fields in Value Objects or Workflow inputs;
- aggregate-aware formatting when rows contain multiple currencies.
