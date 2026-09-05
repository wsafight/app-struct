import DecimalLibrary from "decimal.js";
import { z } from "zod";
import type { FieldDefinition, FieldKind } from "./resource";

export type FormValue = string | boolean;
export type FormValues = Record<string, FormValue>;
export type ValueField = Pick<
  FieldDefinition,
  "name" | "label" | "kind" | "required"
> &
  Partial<Pick<FieldDefinition, "minimum" | "maximum" | "values">>;

const INTEGER = /^-?\d+$/;
const Decimal = DecimalLibrary.clone({ precision: 80 });
const DECIMAL = /^-?(?:\d+(?:\.\d*)?|\.\d+)$/;
const LOCAL_DATETIME =
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(?::\d{2}(?:\.\d{1,6})?)?$/;

export function inputType(kind: FieldKind): string {
  if (kind === "integer") return "number";
  if (kind === "date") return "date";
  if (kind === "datetime") return "datetime-local";
  return "text";
}

function localDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  const pad = (number: number) => String(number).padStart(2, "0");
  const fraction = /\.(\d+)/.exec(value)?.[1];
  return `${String(date.getFullYear()).padStart(4, "0")}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}${fraction ? `.${fraction}` : ""}`;
}

function utcDateTime(value: string): string {
  if (!LOCAL_DATETIME.test(value))
    throw new Error("Invalid local date and time");
  const date = new Date(value);
  if (Number.isNaN(date.getTime()))
    throw new Error("Invalid local date and time");
  const normalized = value.length === 16 ? `${value}:00` : value;
  const fraction = /\.(\d+)/.exec(value)?.[1];
  const utc = date
    .toISOString()
    .replace(/\.\d{3}Z$/, fraction ? `.${fraction}Z` : "Z");
  // Date normalizes invalid calendar values and times inside a daylight-saving gap.
  if (localDateTime(utc) !== normalized)
    throw new Error("Invalid local date and time");
  return utc;
}

export function toFormValue(value: unknown, field: ValueField): FormValue {
  if (field.kind === "boolean") return Boolean(value);
  if (field.kind === "json")
    return value == null ? "" : JSON.stringify(value, null, 2);
  if (field.kind === "datetime" && typeof value === "string")
    return localDateTime(value);
  return value == null ? "" : String(value);
}

export function valueError(
  value: FormValue | undefined,
  field: ValueField,
): string | undefined {
  if (value === "" || value === undefined)
    return field.required ? `${field.label} is required` : undefined;
  const text = String(value);
  if (field.kind === "boolean")
    return typeof value === "boolean"
      ? undefined
      : `${field.label} must be a boolean`;
  if (["integer", "bigint", "decimal"].includes(field.kind)) {
    if (!(field.kind === "decimal" ? DECIMAL : INTEGER).test(text))
      return `${field.label} must be ${field.kind === "decimal" ? "a decimal number" : "a whole number"}`;
    const number = new Decimal(text);
    const minimum =
      field.kind === "integer" ? "-2147483648" : "-9223372036854775808";
    const maximum =
      field.kind === "integer" ? "2147483647" : "9223372036854775807";
    if (field.kind !== "decimal" && (number.lt(minimum) || number.gt(maximum)))
      return `${field.label} is outside the ${field.kind} range`;
    if (
      field.kind === "decimal" &&
      (number.decimalPlaces() > 28 ||
        number
          .abs()
          .mul(new Decimal(10).pow(number.decimalPlaces()))
          .gt("79228162514264337593543950335"))
    )
      return `${field.label} exceeds decimal precision`;
    if (field.minimum !== undefined && number.lt(field.minimum))
      return `${field.label} must be at least ${field.minimum}`;
    if (field.maximum !== undefined && number.gt(field.maximum))
      return `${field.label} must be at most ${field.maximum}`;
  }
  if (field.kind === "uuid" && !z.uuid().safeParse(text).success)
    return `${field.label} must be a valid UUID`;
  if (field.kind === "enum" && field.values && !field.values.includes(text))
    return `${field.label} must be one of the configured values`;
  if (field.kind === "json") {
    try {
      JSON.parse(text);
    } catch {
      return `${field.label} must contain valid JSON`;
    }
  }
  if (field.kind === "datetime") {
    try {
      utcDateTime(text);
    } catch {
      return `${field.label} must be a valid local date and time`;
    }
  }
  if (field.kind === "date") {
    const date = new Date(`${text}T00:00:00Z`);
    if (
      !/^\d{4}-\d{2}-\d{2}$/.test(text) ||
      Number.isNaN(date.getTime()) ||
      date.toISOString().slice(0, 10) !== text
    )
      return `${field.label} must be a valid date`;
  }
  return undefined;
}

export function fieldSchema(
  field: ValueField,
): z.ZodType<FormValue, FormValue> {
  return z.union([z.string(), z.boolean()]).superRefine((value, context) => {
    const message = valueError(value, field);
    if (message) context.addIssue({ code: "custom", message });
  });
}

export function toApiValue(
  value: FormValue | undefined,
  field: ValueField,
  original?: unknown,
): unknown {
  const error = valueError(value, field);
  if (error) throw new Error(error);
  if (value === "" || value === undefined) return null;
  if (field.kind === "integer") return Number(value);
  if (field.kind === "bigint") return BigInt(String(value)).toString();
  if (field.kind === "decimal") return new Decimal(String(value)).toFixed();
  if (field.kind === "json") return JSON.parse(String(value));
  if (field.kind === "datetime") {
    if (typeof original === "string" && toFormValue(original, field) === value)
      return original;
    return utcDateTime(String(value));
  }
  return value;
}

export function formatMoney(
  amount: unknown,
  currency: string,
  fractionDigits: number,
): string {
  const decimal = new Decimal(String(amount));
  if (!decimal.isFinite()) throw new Error("Invalid monetary amount");
  const formatter = new Intl.NumberFormat(undefined, {
    style: "currency",
    currency,
    currencyDisplay: "code",
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  });
  // ECMA-402 accepts exact decimal strings; TypeScript's Intl signature omits this overload.
  return formatter.format(
    decimal.toFixed(fractionDigits, Decimal.ROUND_HALF_UP) as unknown as number,
  );
}
