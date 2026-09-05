import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { test } from "node:test";
import { PDFDocument } from "pdf-lib";
import { digest, render, validate } from "./render.mjs";

export function request(html, overrides = {}) {
  return { protocol: 1, request_id: randomUUID(), run_id: randomUUID(), tenant_id: randomUUID(), renderer: "chromium-v1", template: "acceptance", template_version: 1,
    artifact_digest: `sha256:${digest(html)}`, html, html_sha256: digest(html), locale: "zh-CN", timezone: "Asia/Shanghai", paper: "a4", orientation: "portrait", deadline_ms: Date.now() + 30_000, ...overrides };
}

test("rejects external resources and active content before browser launch", () => {
  for (const html of [
    '<img src="http://169.254.169.254/latest/meta-data/">', '<img src="http://127.0.0.1:8888/">',
    '<img src="http://192.168.1.1/">', '<img src="https://redirect.example.test/">',
    '<img src="file:///etc/passwd">', '<iframe src="about:blank"></iframe>',
    '<style>@import "https://example.test/x.css";</style>', '<div style="background:url(file:///etc/passwd)"></div>',
    '<style>@font-face { font-family: secret; src: url(http://localhost/font); }</style>',
    '<script>fetch("http://localhost")</script>', '<img srcset="http://localhost/a 2x">',
    '<meta http-equiv="refresh" content="0;url=http://localhost">',
  ]) assert.throws(() => validate(request(html)), /REPORT_BLOCKED_RESOURCE/);
  assert.throws(() => validate(request("x".repeat(2 * 1024 * 1024 + 1))), /REPORT_RESOURCE_LIMIT/);
  assert.throws(() => validate(request("ok", { deadline_ms: Date.now() - 1 })), /REPORT_RENDER_TIMEOUT/);
  assert.throws(() => validate(request("ok", { html_sha256: "wrong" })), /REPORT_INVALID_TEMPLATE_ARTIFACT/);
});

test("renders Chinese text, embedded bitmap, multiple pages and landscape", { timeout: 45_000 }, async () => {
  const image = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9ZQmcAAAAASUVORK5CYII=";
  const html = `<!doctype html><html lang="zh-CN"><head><style>
    body { font-family: "Noto Sans CJK SC", "PingFang SC", sans-serif; margin: 32px; color: #182026; }
    h1 { font-size: 28px; } table { border-collapse: collapse; width: 100%; }
    th, td { border-bottom: 1px solid #d9e0e4; padding: 12px; text-align: left; }
    .next { break-before: page; } img { width: 24px; height: 24px; }
    </style></head><body><h1>Operations Report</h1><p>\u91c7\u8d2d\u660e\u7ec6 / 2026-09-06</p>
    <img alt="Embedded bitmap" src="data:image/png;base64,${image}">
    <table><thead><tr><th>\u5546\u54c1</th><th>\u6570\u91cf</th><th>\u91d1\u989d</th></tr></thead>
    <tbody><tr><td>\u529e\u516c\u7528\u54c1</td><td>2</td><td>CNY 19.95</td></tr></tbody></table>
    <div class="next"><h1>Approval</h1><p>\u5ba1\u6838\u901a\u8fc7</p></div></body></html>`;
  const result = await render(request(html, { orientation: "landscape" }), new AbortController().signal);
  const bytes = Buffer.from(result.pdf, "base64");
  assert.equal(result.pages, 2);
  assert.equal(result.byte_length, bytes.length);
  assert.equal(result.sha256, digest(bytes));
  const pdf = await PDFDocument.load(bytes);
  assert.ok(pdf.getPage(0).getWidth() > pdf.getPage(0).getHeight());
  if (process.env.APPSTRUCT_RENDERER_TEST_OUTPUT) {
    await mkdir(process.env.APPSTRUCT_RENDERER_TEST_OUTPUT, { recursive: true });
    await writeFile(`${process.env.APPSTRUCT_RENDERER_TEST_OUTPUT}/renderer-acceptance.pdf`, bytes);
  }
});

test("cancellation and deadlines terminate browser work", { timeout: 30_000 }, async () => {
  const abort = new AbortController();
  const pending = render(request("<p>cancel</p>"), abort.signal);
  abort.abort();
  await assert.rejects(pending, /REPORT_CANCELLED/);
  await assert.rejects(render(request("<p>timeout</p>", { deadline_ms: Date.now() + 1 }), new AbortController().signal), /REPORT_RENDER_TIMEOUT/);
  const result = await render(request("<p>recovered</p>"), new AbortController().signal);
  assert.equal(result.pages, 1);
});

test("rejects documents beyond the page budget", { timeout: 30_000 }, async () => {
  const html = Array.from({ length: 101 }, (_, index) => `<div style="break-before:page">${index}</div>`).join("");
  await assert.rejects(render(request(html), new AbortController().signal), /REPORT_RESOURCE_LIMIT/);
});
