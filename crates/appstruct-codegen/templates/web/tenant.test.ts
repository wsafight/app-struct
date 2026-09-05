import { describe, expect, it } from "vitest";
import { tenantApi } from "./generated/client";

describe("tenant selection", () => {
  it("continues when browser storage is unavailable", () => {
    const descriptor = Object.getOwnPropertyDescriptor(window, "localStorage");
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      get: () => {
        throw new DOMException("Storage is disabled", "SecurityError");
      },
    });
    try {
      expect(tenantApi.current()).toBeUndefined();
      expect(() => tenantApi.select("tenant-1")).not.toThrow();
      expect(() => tenantApi.clear()).not.toThrow();
    } finally {
      if (descriptor) Object.defineProperty(window, "localStorage", descriptor);
      else delete (window as { localStorage?: Storage }).localStorage;
    }
  });
});
