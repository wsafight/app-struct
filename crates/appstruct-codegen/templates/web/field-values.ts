import { uiLocale, t } from "./ui";
import DecimalLibrary from "decimal.js";
import type { FieldDefinition, FieldKind } from "./resource";

export type FormValue = string | boolean;
export type FormValues = Record<string, FormValue>;
export type ValueField = Pick<
  FieldDefinition,
  "name" | "label" | "kind" | "required"
> &
  Partial<
    Pick<
      FieldDefinition,
      "minimum" | "maximum" | "values" | "minLength" | "maxLength"
    >
  >;

const INTEGER = /^-?\d+$/;
// RFC 9562 UUID versions 1–8, including the nil and max UUIDs.
const UUID =
  /^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$/;
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
    throw new Error(t("Invalid local date and time"));
  const date = new Date(value);
  if (Number.isNaN(date.getTime()))
    throw new Error(t("Invalid local date and time"));
  const normalized = value.length === 16 ? `${value}:00` : value;
  const fraction = /\.(\d+)/.exec(value)?.[1];
  const utc = date
    .toISOString()
    .replace(/\.\d{3}Z$/, fraction ? `.${fraction}Z` : "Z");
  // Date normalizes invalid calendar values and times inside a daylight-saving gap.
  if (localDateTime(utc) !== normalized)
    throw new Error(t("Invalid local date and time"));
  return utc;
}

export function toFormValue(value: unknown, field: ValueField): FormValue {
  if (field.kind === "boolean")
    return value == null && !field.required ? "" : Boolean(value);
  if (field.kind === "json")
    return value == null ? "" : JSON.stringify(value, null, 2);
  if (field.kind === "datetime" && typeof value === "string")
    // Native controls accept milliseconds and normalize zero seconds/fractions.
    // Keep the original instant in the baseline so unchanged edits retain micros.
    return localDateTime(value)
      .replace(/\.(\d+)$/, (_, fraction: string) => {
        const milliseconds = fraction.slice(0, 3).replace(/0+$/, "");
        return milliseconds ? `.${milliseconds}` : "";
      })
      .replace(/T(\d{2}:\d{2}):00$/, "T$1");
  return value == null ? "" : String(value);
}

export function valueError(
  value: FormValue | undefined,
  field: ValueField,
): string | undefined {
  if (value === "" || value === undefined)
    return field.required
      ? t("{0} is required", { "0": field.label })
      : undefined;
  const text = String(value);
  if (["string", "text"].includes(field.kind)) {
    const length = Array.from(text).length;
    if (field.minLength !== undefined && length < field.minLength)
      return t("{0} must contain at least {1} characters", {
        0: field.label,
        1: field.minLength,
      });
    if (field.maxLength !== undefined && length > field.maxLength)
      return t("{0} must contain at most {1} characters", {
        0: field.label,
        1: field.maxLength,
      });
  }
  if (field.kind === "boolean")
    return typeof value === "boolean"
      ? undefined
      : t("{0} must be a boolean", { "0": field.label });
  if (["integer", "bigint", "decimal"].includes(field.kind)) {
    if (!(field.kind === "decimal" ? DECIMAL : INTEGER).test(text))
      return t("{0} must be {1}", {
        "0": field.label,
        "1":
          field.kind === "decimal"
            ? t("a decimal number")
            : t("a whole number"),
      });
    const number = new Decimal(text);
    const minimum =
      field.kind === "integer" ? "-2147483648" : "-9223372036854775808";
    const maximum =
      field.kind === "integer" ? "2147483647" : "9223372036854775807";
    if (field.kind !== "decimal" && (number.lt(minimum) || number.gt(maximum)))
      return t("{0} is outside the {1} range", {
        0: field.label,
        1: field.kind,
      });
    if (
      field.kind === "decimal" &&
      (number.decimalPlaces() > 28 ||
        number
          .abs()
          .mul(new Decimal(10).pow(number.decimalPlaces()))
          .gt("79228162514264337593543950335"))
    )
      return t("{0} exceeds decimal precision", { 0: field.label });
    if (field.minimum !== undefined && number.lt(field.minimum))
      return t("{0} must be at least {1}", {
        "0": field.label,
        "1": field.minimum,
      });
    if (field.maximum !== undefined && number.gt(field.maximum))
      return t("{0} must be at most {1}", {
        "0": field.label,
        "1": field.maximum,
      });
  }
  if (field.kind === "uuid" && !UUID.test(text))
    return t("{0} must be a valid UUID", { "0": field.label });
  if (field.kind === "enum" && field.values && !field.values.includes(text))
    return t("{0} must be one of the configured values", { "0": field.label });
  if (field.kind === "json") {
    try {
      JSON.parse(text);
    } catch {
      return t("{0} must contain valid JSON", { "0": field.label });
    }
  }
  if (field.kind === "datetime") {
    try {
      utcDateTime(text);
    } catch {
      return t("{0} must be a valid local date and time", { "0": field.label });
    }
  }
  if (field.kind === "date") {
    const date = new Date(`${text}T00:00:00Z`);
    if (
      !/^\d{4}-\d{2}-\d{2}$/.test(text) ||
      Number.isNaN(date.getTime()) ||
      date.toISOString().slice(0, 10) !== text
    )
      return t("{0} must be a valid date", { "0": field.label });
  }
  return undefined;
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

/** Render an arbitrary field value for read-only display. */
export function formatValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "boolean") return value ? t("Yes") : t("No");
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

export function formatMoney(
  amount: unknown,
  currency: string,
  fractionDigits: number,
): string {
  const decimal = new Decimal(String(amount));
  if (!decimal.isFinite()) throw new Error(t("Invalid monetary amount"));
  // `Intl.NumberFormat` construction resolves locale data and is far more expensive
  // than formatting, so reuse one instance per currency and precision. This runs once
  // per money cell on every render, which makes the cache the difference between one
  // formatter and one per cell.
  const formatter = moneyFormatter(currency, fractionDigits);
  // ECMA-402 accepts exact decimal strings; TypeScript's Intl signature omits this overload.
  return formatter.format(
    decimal.toFixed(fractionDigits, Decimal.ROUND_HALF_UP) as unknown as number,
  );
}

const moneyFormatters = new Map<string, Intl.NumberFormat>();

function moneyFormatter(
  currency: string,
  fractionDigits: number,
): Intl.NumberFormat {
  const key = `${uiLocale()}\u0000${currency}\u0000${fractionDigits}`;
  const cached = moneyFormatters.get(key);
  if (cached) return cached;
  const formatter = new Intl.NumberFormat(uiLocale(), {
    style: "currency",
    currency,
    currencyDisplay: "code",
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  });
  moneyFormatters.set(key, formatter);
  return formatter;
}

/** Validate a complete candidate, including unchanged values on partial edits. */
export function recordRuleErrors(
  record: Record<string, unknown>,
  fields: FieldDefinition[],
): Record<string, string> {
  const errors: Record<string, string> = {};
  for (const field of fields) {
    const value = record[field.name];
    if (
      field.requiredWhen &&
      record[field.requiredWhen.field] === field.requiredWhen.equals &&
      (value === null || value === undefined || value === "")
    ) {
      errors[field.name] = t("{0} is required", { 0: field.label });
    }
    const other = field.greaterThan && record[field.greaterThan];
    if (
      !field.greaterThan ||
      value == null ||
      value === "" ||
      other == null ||
      other === ""
    )
      continue;
    try {
      const greater =
        field.kind === "datetime"
          ? instantMicros(String(value)) > instantMicros(String(other))
          : field.kind === "date"
            ? String(value) > String(other)
            : new Decimal(String(value)).gt(String(other));
      if (!greater)
        errors[field.name] = t("{0} must be greater than {1}", {
          0: field.label,
          1:
            fields.find((item) => item.name === field.greaterThan)?.label ??
            field.greaterThan,
        });
    } catch {
      /* Scalar validation reports invalid values separately. */
    }
  }
  return errors;
}

function instantMicros(value: string): bigint {
  const milliseconds = Date.parse(value);
  if (!Number.isFinite(milliseconds)) throw new Error("Invalid date and time");
  const fraction = /\.(\d+)(?:Z|[+-]\d{2}:\d{2})$/.exec(value)?.[1] ?? "";
  return (
    BigInt(milliseconds) * 1000n + BigInt(fraction.padEnd(6, "0").slice(3, 6))
  );
}
