import { createHash } from "node:crypto";
import { chromium } from "playwright";
import { PDFDocument } from "pdf-lib";
import { validateResources } from "./resources.mjs";

export const MAX_REQUEST = 4 * 1024 * 1024;
const MAX_HTML = 2 * 1024 * 1024;
const MAX_PDF = 50 * 1024 * 1024;
export const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fail = (code) => { throw new Error(code); };

export function validate(request) {
  if (!request || request.protocol !== 1 || request.renderer !== "chromium-v1"
    || ![request.request_id, request.run_id].every((id) => typeof id === "string" && /^[0-9a-f-]{36}$/.test(id))
    || typeof request.artifact_digest !== "string" || !/^sha256:[0-9a-f]{64}$/.test(request.artifact_digest)
    || typeof request.html !== "string" || request.html_sha256 !== digest(request.html)
    || typeof request.locale !== "string" || typeof request.timezone !== "string"
    || !["a4", "a3", "letter", "legal"].includes(request.paper)
    || !["portrait", "landscape"].includes(request.orientation)) fail("REPORT_INVALID_TEMPLATE_ARTIFACT");
  if (Buffer.byteLength(request.html) > MAX_HTML) fail("REPORT_RESOURCE_LIMIT");
  if (!Number.isSafeInteger(request.deadline_ms) || request.deadline_ms <= Date.now()) fail("REPORT_RENDER_TIMEOUT");
  validateResources(request.html);
}

export async function render(request, signal) {
  validate(request);
  let browser;
  let timer;
  let abort;
  let finished = false;
  const cancelled = new Promise((_, reject) => {
    abort = () => { finished = true; reject(new Error("REPORT_CANCELLED")); void browser?.close(); };
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
    timer = setTimeout(() => { finished = true; reject(new Error("REPORT_RENDER_TIMEOUT")); void browser?.close(); }, Math.min(30_000, request.deadline_ms - Date.now()));
  });
  const operation = async () => {
    browser = await chromium.launch({ chromiumSandbox: true, headless: true, timeout: Math.max(1, Math.min(10_000, request.deadline_ms - Date.now())), args: ["--disable-dev-shm-usage"] });
    if (finished) { await browser.close(); fail("REPORT_CANCELLED"); }
    const context = await browser.newContext({ javaScriptEnabled: false, serviceWorkers: "block", acceptDownloads: false, locale: request.locale, timezoneId: request.timezone });
    let blocked = false;
    await context.route("**/*", (route) => { blocked = true; return route.abort("blockedbyclient"); });
    const page = await context.newPage();
    await page.emulateMedia({ media: "print" });
    const policy = `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; font-src data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">`;
    await page.setContent(policy + request.html, { waitUntil: "load", timeout: 15_000 });
    if (blocked) fail("REPORT_BLOCKED_RESOURCE");
    await page.evaluate(() => document.fonts.ready);
    if (!await page.evaluate(() => [...document.images].every((image) => image.complete && image.naturalWidth > 0))) fail("REPORT_INVALID_OUTPUT");
    const pdf = await page.pdf({ format: request.paper.toUpperCase(), landscape: request.orientation === "landscape", printBackground: true, preferCSSPageSize: false });
    if (blocked) fail("REPORT_BLOCKED_RESOURCE");
    if (pdf.length > MAX_PDF) fail("REPORT_RESOURCE_LIMIT");
    const document = await PDFDocument.load(pdf, { updateMetadata: false });
    const pages = document.getPageCount();
    if (pages < 1 || pages > 100) fail("REPORT_RESOURCE_LIMIT");
    return { pdf: pdf.toString("base64"), sha256: digest(pdf), byte_length: pdf.length, pages };
  };
  const work = operation();
  try { return await Promise.race([work, cancelled]); }
  finally {
    finished = true;
    clearTimeout(timer);
    signal.removeEventListener("abort", abort);
    await browser?.close();
    await work.catch(() => {});
  }
}
