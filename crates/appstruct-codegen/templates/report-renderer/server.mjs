import net from "node:net";
import { chmod, lstat, unlink } from "node:fs/promises";
import { MAX_REQUEST, render } from "./render.mjs";

const path = process.env.APPSTRUCT_RENDERER_SOCKET ?? "/run/appstruct-renderer/renderer.sock";
try {
  const stat = await lstat(path);
  if (!stat.isSocket()) throw new Error("Socket path is occupied");
  await unlink(path);
} catch (error) { if (error.code !== "ENOENT") throw error; }
let active = false;
const sockets = new Set();
const server = net.createServer((socket) => {
  sockets.add(socket);
  const abort = new AbortController();
  let pending = Buffer.alloc(0);
  let size;
  let started = false;
  const deadline = setTimeout(() => socket.destroy(), 35_000);
  socket.setTimeout(35_000, () => socket.destroy());
  socket.on("error", () => {});
  socket.on("close", () => { clearTimeout(deadline); sockets.delete(socket); abort.abort(); });
  socket.on("data", (chunk) => {
    if (started) { socket.destroy(); return; }
    pending = Buffer.concat([pending, chunk]);
    if (pending.length > MAX_REQUEST + 4) { socket.destroy(); return; }
    if (size === undefined && pending.length >= 4) {
      size = pending.readUInt32BE(0);
      if (size === 0 || size > MAX_REQUEST) { socket.destroy(); return; }
    }
    if (size === undefined || pending.length < size + 4) return;
    if (pending.length !== size + 4) { socket.destroy(); return; }
    started = true;
    let request;
    try { request = JSON.parse(pending.subarray(4)); }
    catch { socket.destroy(); return; }
    pending = Buffer.alloc(0);
    const identity = { protocol: 1, request_id: request.request_id, run_id: request.run_id, renderer: "chromium-v1", artifact_digest: request.artifact_digest, html_sha256: request.html_sha256 };
    const send = (result) => {
      if (socket.destroyed) return;
      const bytes = Buffer.from(JSON.stringify({ ...identity, ...result }));
      const header = Buffer.alloc(4); header.writeUInt32BE(bytes.length);
      socket.end(Buffer.concat([header, bytes]));
    };
    if (active) { send({ code: "REPORT_ADAPTER_UNAVAILABLE" }); return; }
    active = true;
    render(request, abort.signal).then((result) => send({ code: "OK", ...result }), (error) => {
      const allowed = new Set(["REPORT_INVALID_TEMPLATE_ARTIFACT", "REPORT_BLOCKED_RESOURCE", "REPORT_RESOURCE_LIMIT", "REPORT_RENDER_TIMEOUT", "REPORT_CANCELLED", "REPORT_INVALID_OUTPUT"]);
      send({ code: allowed.has(error.message) ? error.message : "REPORT_BROWSER_CRASH" });
    }).finally(() => { active = false; });
  });
});
server.maxConnections = 8;
await new Promise((resolve, reject) => { server.once("error", reject); server.listen(path, resolve); });
await chmod(path, 0o660);
for (const signal of ["SIGTERM", "SIGINT"]) process.on(signal, () => {
  for (const socket of sockets) socket.destroy();
  server.close(() => { void unlink(path).finally(() => process.exit(0)); });
});
