import assert from "node:assert/strict";

const base = process.argv[2]?.replace(/\/$/, "");
assert(base && ["http:", "https:"].includes(new URL(base).protocol), "Usage: node deploy/smoke.mjs https://app.example.com");
const get = (path) => fetch(`${base}${path}`, { signal: AbortSignal.timeout(10000), redirect: "error" });
for (const path of ["/health/live", "/health/ready"]) {
  const response = await get(path);
  assert.equal(response.status, 204, `${path} failed`);
  assert(response.headers.get("x-request-id"), `${path} has no request ID`);
}
const schemaResponse = await get("/openapi.json");
assert(schemaResponse.ok && schemaResponse.headers.get("content-type")?.includes("application/json"));
const schema = await schemaResponse.json();
assert(schema.openapi.startsWith("3.") && Object.keys(schema.paths).some((path) => path.startsWith("/api/")));
const index = await get("/");
assert(index.ok && index.headers.get("content-type")?.includes("text/html"));
const html = await index.text();
assert(html.includes('id="root"'), "Web application shell is missing");
const fallback = await get("/__appstruct_smoke_spa__");
assert.equal(await fallback.text(), html, "SPA fallback failed");
for (const path of ["/api/__appstruct_smoke_missing__", "/assets/__appstruct_missing__.js", "/metrics"]) {
  const response = await get(path);
  assert.equal(response.status, 404, `${path} must return 404`);
  await response.arrayBuffer();
}
console.log(`Deployment probes passed for ${base}`);
