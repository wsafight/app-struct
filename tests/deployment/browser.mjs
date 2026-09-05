import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { chromium } from "@playwright/test";

const [base, output] = process.argv.slice(2);
const headers = { "Content-Type": "application/json" };
async function json(path, init) {
  const response = await fetch(`${base}${path}`, { ...init, signal: AbortSignal.timeout(10000) });
  assert(response.ok, `HTTP ${response.status} for ${path}`);
  return response.json();
}
const created = await json("/api/notes/", { method: "POST", headers,
  body: JSON.stringify({ title: "Deployment acceptance", body: "Production bundle and database" }) });
const changed = await json(`/api/notes/${created.id}`, { method: "PATCH",
  headers: { ...headers, "If-Match": `"rev-${created.revision}"` }, body: JSON.stringify({ body: "Revision checked" }) });
assert.equal(changed.body, "Revision checked");
const browser = await chromium.launch();
try {
  await mkdir(output, { recursive: true });
  for (const [name, viewport] of [["desktop", { width: 1440, height: 1000 }], ["mobile", { width: 390, height: 844 }]]) {
    const page = await browser.newPage({ viewport });
    const errors = [];
    const apiOrigins = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("request", (request) => {
      const url = new URL(request.url());
      if (url.pathname.startsWith("/api/")) apiOrigins.push(url.origin);
    });
    await page.goto(base);
    try {
      await page.getByText("Deployment acceptance", { exact: true }).waitFor();
    } catch (error) {
      await page.screenshot({ path: `${output}/${name}-failure.png`, fullPage: true });
      console.error(await page.locator("body").innerText(), errors);
      throw error;
    }
    assert(apiOrigins.length > 0 && apiOrigins.every((origin) => origin === new URL(base).origin));
    assert.equal(errors.length, 0, errors.join("\n"));
    await page.screenshot({ path: `${output}/${name}.png`, fullPage: true });
    const layout = await page.evaluate(() => ({
      width: document.documentElement.scrollWidth, viewport: window.innerWidth,
      outside: [...document.querySelectorAll("body *")].filter((element) => element.getBoundingClientRect().right > window.innerWidth + 1)
        .slice(0, 8).map((element) => `${element.tagName}.${element.className}`),
    }));
    assert(layout.width <= layout.viewport, `Page overflows viewport: ${JSON.stringify(layout)}`);
    await page.close();
  }
} finally { await browser.close(); }
const deleted = await fetch(`${base}/api/notes/${created.id}`, { method: "DELETE",
  headers: { "If-Match": `"rev-${changed.revision}"` }, signal: AbortSignal.timeout(10000) });
assert(deleted.ok);
await deleted.arrayBuffer();
assert.equal((await fetch(`${base}/api/notes/${created.id}`)).status, 404);
console.log("Production Web desktop/mobile, same-origin API and revision-checked CRUD passed");
