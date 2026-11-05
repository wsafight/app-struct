import { createServer } from "node:http";
import { readFile, writeFile, mkdir, stat } from "node:fs/promises";
import { resolve, extname, dirname, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const workspace = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(
  process.argv[2] ??
    resolve(workspace, "examples/saas-demo/generated/web/dist"),
);
const origin = process.env.APPSTRUCT_PUBLIC_SITE_URL;
if (origin) {
  const url = new URL(origin);
  if (
    url.protocol !== "https:" ||
    url.pathname !== "/" ||
    url.search ||
    url.hash ||
    url.username ||
    url.password
  )
    throw new Error(
      "APPSTRUCT_PUBLIC_SITE_URL must be an HTTPS origin without credentials, path, query or fragment",
    );
}
const mime = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
};
const server = createServer(async (request, response) => {
  try {
    const pathname = decodeURIComponent(
      new URL(request.url, "http://localhost").pathname,
    );
    if (pathname.startsWith("/api/")) {
      response.writeHead(401, { "Content-Type": "application/json" });
      response.end(
        JSON.stringify({
          error: {
            code: "UNAUTHORIZED",
            message: "Anonymous build",
            fields: [],
          },
        }),
      );
      return;
    }
    const candidate = resolve(root, `.${pathname}`);
    if (candidate !== root && !candidate.startsWith(`${root}${sep}`)) {
      response.writeHead(403).end();
      return;
    }
    let file = candidate;
    const info = await stat(file).catch(() => null);
    if (info?.isDirectory()) file = resolve(file, "index.html");
    if (!info) file = resolve(root, "index.html");
    response.writeHead(200, {
      "Content-Type": mime[extname(file)] ?? "application/octet-stream",
    });
    response.end(await readFile(file));
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((accept) => server.listen(0, "127.0.0.1", accept));
let browser;
try {
  browser = await chromium.launch();
  const port = server.address().port;
  const context = await browser.newContext({ serviceWorkers: "block" });
  const page = await context.newPage();
  await page.route("**/*", (route) =>
    new URL(route.request().url()).origin === `http://127.0.0.1:${port}`
      ? route.continue()
      : route.abort(),
  );
  const pages = [
    ["/", "Turn your team's data into a report worth sharing."],
    ["/pricing", "Start small. Make room for more."],
  ];
  const rendered = [];
  for (const [route, heading] of pages) {
    await page.goto(`http://127.0.0.1:${port}${route}`, {
      waitUntil: "networkidle",
    });
    await page.getByRole("heading", { name: heading, exact: true }).waitFor();
    await page.evaluate(
      ({ route, origin }) => {
        if (origin) {
          const canonical = document.createElement("link");
          canonical.rel = "canonical";
          canonical.href = new URL(route, origin).href;
          document.head.append(canonical);
        }
      },
      { route, origin },
    );
    const html = await page.content();
    if (
      !html.includes('<meta name="description"') ||
      (!html.includes(heading.replaceAll("'", "&#x27;")) &&
        !html.includes(heading))
    )
      throw new Error(`Prerender content missing for ${route}`);
    rendered.push([route, html]);
  }
  for (const [route, html] of rendered) {
    const destination =
      route === "/"
        ? resolve(root, "index.html")
        : resolve(root, "pricing/index.html");
    await mkdir(dirname(destination), { recursive: true });
    await writeFile(destination, html);
    console.log(`Prerendered ${route}`);
  }
  await context.close();
  const staticContext = await browser.newContext({
    javaScriptEnabled: false,
    serviceWorkers: "block",
  });
  const staticPage = await staticContext.newPage();
  for (const [route, heading] of pages) {
    await staticPage.goto(`http://127.0.0.1:${port}${route}`);
    await staticPage
      .getByRole("heading", { name: heading, exact: true })
      .waitFor();
    if ((await staticPage.locator('meta[name="description"]').count()) !== 1)
      throw new Error(`Invalid static description for ${route}`);
    if (
      origin &&
      (await staticPage
        .locator('link[rel="canonical"]')
        .getAttribute("href")) !== new URL(route, origin).href
    )
      throw new Error(`Invalid static canonical for ${route}`);
  }
  await staticContext.close();
  await writeFile(
    resolve(root, "robots.txt"),
    `User-agent: *\nDisallow: /\nAllow: /$\nAllow: /pricing$\nAllow: /pricing/$\nAllow: /static/\n${origin ? `Sitemap: ${new URL("/sitemap.xml", origin).href}\n` : ""}`,
  );
  if (origin)
    await writeFile(
      resolve(root, "sitemap.xml"),
      `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${["/", "/pricing"].map((route) => `<url><loc>${new URL(route, origin).href.replaceAll("&", "&amp;")}</loc></url>`).join("")}</urlset>`,
    );
} finally {
  await browser?.close();
  await new Promise((accept, reject) =>
    server.close((error) => (error ? reject(error) : accept())),
  );
}
