import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { cpus, platform, arch } from "node:os";

const [api, output] = process.argv.slice(2);
assert(api && output && process.env.APPSTRUCT_E2E_DATABASE_URL);
function option(name, fallback, maximum) {
  const value = Number(process.env[name] ?? fallback);
  assert(Number.isInteger(value) && value > 0 && value <= maximum, `Invalid ${name}`);
  return value;
}
function concurrencyTiers() {
  const source = process.env.APPSTRUCT_BENCH_CONCURRENCIES
    ?? process.env.APPSTRUCT_BENCH_CONCURRENCY
    ?? "1,8,24";
  const tiers = [...new Set(source.split(",").map(Number))];
  assert(tiers.length > 0 && tiers.length <= 8, "Invalid APPSTRUCT_BENCH_CONCURRENCIES");
  assert(tiers.every((value) => Number.isInteger(value) && value > 0 && value <= 64),
    "Invalid APPSTRUCT_BENCH_CONCURRENCIES");
  return tiers;
}
const profile = process.env.APPSTRUCT_BENCH_PROFILE ?? "release";
assert(["debug", "release"].includes(profile), "Invalid APPSTRUCT_BENCH_PROFILE");
const rows = option("APPSTRUCT_BENCH_ROWS", 25000, 1000000);
const iterations = option("APPSTRUCT_BENCH_ITERATIONS", 200, 10000);
const concurrencies = concurrencyTiers();
const mixedSeconds = option("APPSTRUCT_BENCH_MIXED_SECONDS", 15, 300);
const budgetOverride = process.env.APPSTRUCT_BENCH_P95_MS === undefined
  ? undefined : option("APPSTRUCT_BENCH_P95_MS", 1, 60000);
const cookies = new Map();
let csrf;
async function request(path, init = {}) {
  const response = await fetch(`${api}${path}`, {
    ...init,
    signal: AbortSignal.timeout(15000),
    headers: {
      "Content-Type": "application/json",
      Cookie: [...cookies].map(([key, value]) => `${key}=${value}`).join("; "),
      ...(csrf ? { "X-CSRF-Token": csrf } : {}),
      ...init.headers,
    },
  });
  for (const cookie of response.headers.getSetCookie()) {
    const pair = cookie.split(";")[0];
    const index = pair.indexOf("=");
    cookies.set(pair.slice(0, index), pair.slice(index + 1));
  }
  csrf = cookies.get("appstruct_csrf");
  return response;
}
async function json(path, init) {
  const response = await request(path, init);
  const data = await response.json();
  assert(response.ok, `HTTP ${response.status}: ${data.error?.code ?? "request failed"}`);
  return data;
}
const account = await json("/api/auth/register", {
  method: "POST", body: JSON.stringify({ email: "benchmark@example.test", password: "AppStruct-Benchmark-2026" }),
});
async function tenant(name) {
  return (await json("/api/tenant/organizations", { method: "POST", body: JSON.stringify({ name }) })).id;
}
const alpha = await tenant("Benchmark Alpha");
const beta = await tenant("Benchmark Beta");
for (const id of [alpha, beta, account.user.id]) assert.match(id, /^[a-f0-9-]{36}$/);
function postgres(args) {
  const result = spawnSync("psql", ["-X", "--dbname", process.env.APPSTRUCT_E2E_DATABASE_URL,
    "-v", "ON_ERROR_STOP=1", ...args], { encoding: "utf8" });
  assert.equal(result.status, 0, "PostgreSQL fixture operation failed; check schema and permissions");
  return result.stdout.trim();
}
const postgresVersion = postgres(["-At", "-c", "SHOW server_version"]);
postgres(["-v", `alpha=${alpha}`, "-v", `beta=${beta}`, "-v", `rows=${rows}`,
  "-f", fileURLToPath(new URL("./seed.sql", import.meta.url))]);
const headers = { "X-AppStruct-Tenant": alpha };
const first = await json("/api/entries/?page_size=25", { headers });
assert.equal(first.meta.total, rows);
assert(first.data.every((entry) => entry.tenant_id === alpha && !Object.hasOwn(entry, "secret")));
const sample = first.data[0];
const memberDenied = await request(`/api/entries/${sample.id}`, { headers });
assert.equal(memberDenied.status, 403);
await memberDenied.arrayBuffer();
postgres(["-c", `UPDATE _appstruct_auth_accounts SET roles = '["admin"]'::jsonb WHERE user_id = '${account.user.id}'`]);
const otherTenant = await request(`/api/entries/${sample.id}`, { headers: { "X-AppStruct-Tenant": beta } });
assert.equal(otherTenant.status, 404);
await otherTenant.arrayBuffer();

const phases = [];
function quantile(values, fraction) { return values[Math.max(0, Math.ceil(values.length * fraction) - 1)] ?? 0; }
function latencyBudget(name) {
  if (budgetOverride !== undefined) return budgetOverride;
  const releaseBudget = name === "audited_crud" ? 1000 : name === "mixed_sustained" ? 750 : 500;
  return profile === "release" ? releaseBudget : releaseBudget * 2;
}
async function phase(name, concurrency, operation) {
  for (let i = 0; i < 5; i++) await operation(-i - 1);
  const durations = [];
  const errors = [];
  let next = 0;
  const started = performance.now();
  await Promise.all(Array.from({ length: concurrency }, async () => {
    while (next < iterations) {
      const iteration = next++;
      const start = performance.now();
      try { await operation(iteration); } catch (error) { errors.push(String(error.message)); }
      durations.push(performance.now() - start);
    }
  }));
  const elapsed = performance.now() - started;
  recordPhase(name, concurrency, durations, errors, elapsed, iterations);
}
async function sustainedPhase(name, concurrency, seconds, operation) {
  for (let i = 0; i < 5; i++) await operation(-i - 1);
  const durations = [];
  const errors = [];
  let next = 0;
  const started = performance.now();
  const deadline = started + seconds * 1000;
  await Promise.all(Array.from({ length: concurrency }, async () => {
    while (performance.now() < deadline) {
      const iteration = next++;
      const start = performance.now();
      try { await operation(iteration); } catch (error) { errors.push(String(error.message)); }
      durations.push(performance.now() - start);
    }
  }));
  const elapsed = performance.now() - started;
  assert(durations.length >= concurrency, "Sustained phase completed too few operations");
  recordPhase(name, concurrency, durations, errors, elapsed, durations.length);
}
function recordPhase(name, concurrency, durations, errors, elapsed, operations) {
  durations.sort((a, b) => a - b);
  phases.push({ name, concurrency, operations, errors: errors.length,
    error_examples: [...new Set(errors)].slice(0, 3), error_rate: errors.length / operations,
    operations_per_second: operations / (elapsed / 1000),
    p50_ms: quantile(durations, 0.5), p95_ms: quantile(durations, 0.95),
    p99_ms: quantile(durations, 0.99), max_ms: durations.at(-1),
    p95_budget_ms: latencyBudget(name) });
  console.log(`${name} c${concurrency}: ${phases.at(-1).p95_ms.toFixed(1)} ms p95, ${errors.length}/${operations} errors`);
}
const offsetList = async (i) => {
  const page = 1 + Math.abs(i) % Math.max(1, Math.ceil(rows / 25));
  const data = await json(`/api/entries/?page_size=25&page=${page}&sort=code`, { headers });
  assert.equal(data.meta.total, rows);
  assert(data.data.every((entry) => entry.tenant_id === alpha));
};
const cursorPage = await json("/api/entries/?limit=25", { headers });
const cursorList = async () => {
  const params = new URLSearchParams({ limit: "25" });
  if (cursorPage.meta.next_cursor) params.set("cursor", cursorPage.meta.next_cursor);
  const data = await json(`/api/entries/?${params}`, { headers });
  assert(data.data.length > 0 && data.data.every((entry) => entry.tenant_id === alpha));
};
const aggregateCount = async () => {
  const data = await json("/api/entries/_aggregate?metrics=count", { headers });
  assert.equal(data.data[0].count, rows);
};
const read = async () => {
  const data = await json(`/api/entries/${sample.id}`, { headers });
  assert.equal(data.secret, "restricted-alpha");
};
const auditedCrud = (prefix) => async (i) => {
  const record = await json("/api/entries/", { headers, method: "POST",
    body: JSON.stringify({ code: `${prefix}-${i}`, title: "Benchmark write", secret: "write" }) });
  const changed = await json(`/api/entries/${record.id}`, { method: "PATCH",
    headers: { ...headers, "If-Match": `"rev-${record.revision}"` }, body: JSON.stringify({ title: "Updated" }) });
  assert.equal(changed.title, "Updated");
  const deleted = await request(`/api/entries/${record.id}`, { method: "DELETE",
    headers: { ...headers, "If-Match": `"rev-${changed.revision}"` } });
  assert(deleted.ok);
  await deleted.arrayBuffer();
};
for (const concurrency of concurrencies) {
  await phase("offset_list", concurrency, offsetList);
  await phase("cursor_list", concurrency, cursorList);
  await phase("aggregate_count", concurrency, aggregateCount);
  await phase("read", concurrency, read);
  await phase("audited_crud", concurrency, auditedCrud(`write-c${concurrency}`));
}
const mixedCrud = auditedCrud("mixed-write");
await sustainedPhase("mixed_sustained", Math.max(...concurrencies), mixedSeconds, async (i) => {
  switch (Math.abs(i) % 10) {
    case 0: return mixedCrud(i);
    case 1:
    case 2: return aggregateCount();
    case 3:
    case 4:
    case 5: return cursorList();
    default: return read();
  }
});

for (let i = 0; i < 100; i++) {
  const response = await request(`/not-a-route/secret-${i}?tenant=${alpha}`);
  assert.equal(response.status, 404);
  await response.arrayBuffer();
}
const metrics = await (await request("/metrics")).text();
assert(metrics.includes('route="/api/entries/{id}"'));
assert(metrics.includes('route="unmatched"'));
assert(metrics.includes('status_class="4xx"'));
assert(metrics.includes("appstruct_http_in_flight 0\n"));
assert(metrics.includes("appstruct_http_dropped_observations_total 0\n"));
assert(!metrics.includes(alpha) && !metrics.includes(sample.id) && !metrics.includes("secret-"));
const distinctLabels = metrics.split("\n").filter((line) => line.startsWith("appstruct_http_request_duration_seconds_count{"));
assert(distinctLabels.length < 30, "Request IDs must not create distinct metric labels");
await mkdir(dirname(output), { recursive: true });
await writeFile(output, `${JSON.stringify({ schema_version: 2, generated_at: new Date().toISOString(),
  environment: { platform: platform(), arch: arch(), cpu: cpus()[0]?.model, node: process.version, postgres: postgresVersion, build_profile: profile },
  workload: { rows_per_tenant: rows, tenants: 2, concurrency_tiers: concurrencies, iterations_per_phase: iterations,
    mixed_sustained_seconds: mixedSeconds, warmup_per_phase: 5 },
  metrics_label_sets: distinctLabels.length, phases }, null, 2)}\n`);
await writeFile(`${output}.prom`, metrics);
assert(phases.every((result) => result.errors === 0 && result.p95_ms <= result.p95_budget_ms),
  "Workload failed its correctness or latency budget");
console.log(`PostgreSQL API benchmark passed; results: ${output}`);
