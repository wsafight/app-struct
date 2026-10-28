import { describe, expect, it } from "@rstest/core";
import {
  formatMoney,
  toApiValue,
  toFormValue,
  valueError,
  type ValueField,
} from "./field-values";

const field = (
  kind: ValueField["kind"],
  extra: Partial<ValueField> = {},
): ValueField => ({
  name: "value",
  label: "Value",
  kind,
  required: true,
  ...extra,
});

describe("lossless business scalars", () => {
  it("validates UUID versions and variants, including nil and max identifiers", () => {
    for (const value of [
      "550e8400-e29b-41d4-a716-446655440000",
      "01990000-0000-7000-8000-000000000001",
      "550E8400-E29B-81D4-B716-446655440000",
      "00000000-0000-0000-0000-000000000000",
      "ffffffff-ffff-ffff-ffff-ffffffffffff",
    ]) {
      expect(valueError(value, field("uuid"))).toBeUndefined();
    }
    for (const value of [
      "invalid",
      "550e8400e29b41d4a716446655440000",
      "550e8400-e29b-01d4-a716-446655440000",
      "550e8400-e29b-91d4-a716-446655440000",
      "550e8400-e29b-41d4-0716-446655440000",
    ]) {
      expect(valueError(value, field("uuid"))).toBe(
        "Value must be a valid UUID",
      );
    }
  });

  it("preserves the complete signed bigint range and validates exact bounds", () => {
    for (const value of [
      "-9223372036854775808",
      "9007199254740993",
      "9223372036854775807",
    ]) {
      expect(
        toApiValue(toFormValue(value, field("bigint")), field("bigint")),
      ).toBe(value);
    }
    expect(valueError("9223372036854775808", field("bigint"))).toBeTruthy();
    expect(
      valueError(
        "9007199254740993",
        field("bigint", { maximum: "9007199254740992" }),
      ),
    ).toBeTruthy();
    expect(valueError("1.5", field("bigint"))).toBeTruthy();
    expect(valueError("2147483648", field("integer"))).toBeTruthy();
  });

  it("keeps decimals exact in inputs, validation and formatted money", () => {
    const value = "12345678901234567890.12345678";
    expect(toApiValue(value, field("decimal"))).toBe(value);
    expect(
      valueError(
        "0.10000000000000000001",
        field("decimal", { maximum: "0.1" }),
      ),
    ).toBeTruthy();
    expect(formatMoney("9007199254740993.15", "USD", 2)).toContain(
      "9,007,199,254,740,993.15",
    );
    expect(formatMoney("-0.005", "USD", 2)).toContain("0.01");
    expect(valueError("NaN", field("decimal"))).toBeTruthy();
    expect(valueError("1e50", field("decimal"))).toBeTruthy();
  });

  it("preserves original microseconds through millisecond-only native datetime controls", () => {
    const datetime = field("datetime");
    for (const instant of [
      "2026-09-05T08:00:27Z",
      "2026-01-01T23:59:59.123456Z",
      "2026-10-01T00:00:00.000001Z",
      "2026-10-01T00:00:00.120000Z",
    ]) {
      const local = toFormValue(instant, datetime);
      expect(String(local)).not.toMatch(/Z$/);
      expect(String(local)).not.toMatch(/\.\d{4}/);
      expect(toApiValue(local, datetime, instant)).toBe(instant);
    }
    const precise = "2026-01-01T23:59:59.123456Z";
    const local = toFormValue(precise, datetime);
    expect(toApiValue(local, datetime)).toBe("2026-01-01T23:59:59.123Z");
    const repeated = "2026-11-01T06:30:00Z";
    expect(
      toApiValue(toFormValue(repeated, datetime), datetime, repeated),
    ).toBe(repeated);
  });

  it("rejects invalid calendar dates and preserves optional nulls and false", () => {
    expect(valueError("2026-02-30", field("date"))).toBeTruthy();
    expect(valueError("2026-02-30T10:00", field("datetime"))).toBeTruthy();
    expect(toApiValue("", field("bigint", { required: false }))).toBeNull();
    expect(toApiValue(false, field("boolean"))).toBe(false);
  });
});
