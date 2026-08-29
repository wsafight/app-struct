import { describe, expect, it } from "vitest";
import { ApiError } from "./generated/client";
import { validateResourceSearch } from "./navigation";
import { shouldRetryQuery } from "./query";

describe("validateResourceSearch", () => {
  it("normalizes supported resource search parameters", () => {
    expect(
      validateResourceSearch({
        page: "3",
        page_size: "50",
        sort: "-created_at",
        q: "quarterly report",
        trash: 1,
        "filter[status]": "open",
        "filter[created_at][gte]": "2026-01-01",
      }),
    ).toEqual({
      page: 3,
      page_size: 50,
      sort: "-created_at",
      q: "quarterly report",
      trash: "1",
      "filter[status]": "open",
      "filter[created_at][gte]": "2026-01-01",
    });
  });

  it("omits defaults and rejects unsupported values", () => {
    expect(
      validateResourceSearch({
        page: "1",
        page_size: 25,
        sort: [],
        q: "",
        trash: "true",
        unknown: "value",
        "filter[status]": "",
        "filter[]": "value",
        "filter[broken": "value",
        "filter[created_at][unknown]": "value",
      }),
    ).toEqual({});
  });

  it("rejects invalid pagination bounds", () => {
    expect(validateResourceSearch({ page: "0", page_size: "101" })).toEqual({});
    expect(validateResourceSearch({ page: "1.5", page_size: Number.NaN })).toEqual({});
  });
});

describe("shouldRetryQuery", () => {
  it("retries transient HTTP and network failures", () => {
    expect(shouldRetryQuery(0, new ApiError(429, "RATE_LIMITED", "slow down"))).toBe(true);
    expect(shouldRetryQuery(1, new ApiError(503, "UNAVAILABLE", "try again"))).toBe(true);
    expect(shouldRetryQuery(0, new TypeError("network unavailable"))).toBe(true);
  });

  it("does not retry permanent, exhausted, or programming failures", () => {
    expect(shouldRetryQuery(0, new ApiError(400, "BAD_REQUEST", "invalid"))).toBe(false);
    expect(shouldRetryQuery(2, new ApiError(503, "UNAVAILABLE", "try again"))).toBe(false);
    expect(shouldRetryQuery(0, new Error("unexpected"))).toBe(false);
  });
});
