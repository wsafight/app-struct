import { describe, expect, it } from "@rstest/core";
import { workspacePath } from "../generated/registry";
import { returnTo, safeReturnTo } from "./return-to";

describe("authentication destination", () => {
  it("preserves a local destination through registration links and refresh", () => {
    const destination = "/studio?plan=pro#upload";
    expect(
      returnTo({ searchStr: `?next=${encodeURIComponent(destination)}` }),
    ).toBe(destination);
    expect(
      returnTo({
        state: {
          from: { pathname: "/studio", searchStr: "?plan=pro", hash: "upload" },
        },
      }),
    ).toBe(destination);
  });

  it("rejects external destinations, control characters and auth loops", () => {
    for (const value of [
      "https://example.test",
      "//example.test",
      "/\\example.test",
      "/login",
      "/register?next=/login",
      "/./login",
      "/api/auth/logout",
      "/studio\n",
    ]) {
      expect(safeReturnTo(value)).toBe(workspacePath);
    }
  });
});
