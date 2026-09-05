import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import http from "node:http";
import { resolve, sep } from "node:path";

// Native acceptance exercises the production bundle; Linux CI exercises the actual nginx image.
const [root, api, port] = process.argv.slice(2);
const assets = resolve(root);
const server = http.createServer(async (request, response) => {
  const path = new URL(request.url, "http://localhost").pathname;
  if (path === "/metrics") { response.writeHead(404).end(); return; }
  if (path.startsWith("/api/") || ["/openapi.json", "/health/live", "/health/ready"].includes(path)) {
    const upstream = http.request(new URL(request.url, api), {
      method: request.method, headers: request.headers,
    }, (result) => { response.writeHead(result.statusCode, result.headers); result.pipe(response); });
    upstream.on("error", () => response.writeHead(502).end());
    request.pipe(upstream);
    return;
  }
  let file = resolve(assets, `.${path}`);
  if (!file.startsWith(`${assets}${sep}`)) file = resolve(assets, "index.html");
  const info = await stat(file).catch(() => null);
  if (!info?.isFile()) {
    if (path.startsWith("/assets/")) { response.writeHead(404).end(); return; }
    file = resolve(assets, "index.html");
  }
  response.setHeader("Content-Type", file.endsWith(".js") ? "text/javascript" : file.endsWith(".css") ? "text/css" : "text/html");
  createReadStream(file).on("error", () => response.destroy()).pipe(response);
});
server.listen(Number(port), "127.0.0.1");
process.on("SIGTERM", () => server.close());
