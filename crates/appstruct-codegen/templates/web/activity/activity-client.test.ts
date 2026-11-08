import { expect, it } from "@rstest/core";
import { ApiError, activityApi } from "../generated/client";

it("uses the shared safe error parser for activity downloads", async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () =>
    new Response(JSON.stringify({ error: null }), { status: 500 });
  try {
    const error = await activityApi
      .download(__ACTIVITY_RESOURCE__, "record-1", "entry-1")
      .catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      status: 500,
      code: "HTTP_ERROR",
      message: "Request failed with status 500",
    });
  } finally {
    globalThis.fetch = originalFetch;
  }
});
