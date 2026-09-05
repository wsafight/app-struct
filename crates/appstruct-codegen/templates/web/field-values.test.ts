import { describe, expect, it } from "vitest";
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

  it("round trips UTC instants through local controls without losing seconds or microseconds", () => {
    const datetime = field("datetime");
    for (const instant of [
      "2026-09-05T08:00:27Z",
      "2026-01-01T23:59:59.123456Z",
    ]) {
      const local = toFormValue(instant, datetime);
      expect(String(local)).not.toMatch(/Z$/);
      expect(toApiValue(local, datetime)).toBe(instant);
    }
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
