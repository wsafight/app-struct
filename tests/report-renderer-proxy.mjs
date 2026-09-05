import net from "node:net";
import { access } from "node:fs/promises";
import { setTimeout as pause } from "node:timers/promises";

const [source, target, gate] = process.argv.slice(2);
const sockets = new Set();
const server = net.createServer(async (socket) => {
  sockets.add(socket);
  socket.on("error", () => {});
  socket.on("close", () => sockets.delete(socket));
  socket.pause();
  while (!socket.destroyed) {
    try { await access(gate); break; } catch { await pause(20); }
  }
  if (socket.destroyed) return;
  const renderer = net.connect(target);
  renderer.on("error", () => socket.destroy());
  socket.on("close", () => renderer.destroy());
  renderer.on("close", () => socket.destroy());
  socket.pipe(renderer).pipe(socket);
});
server.listen(source);
process.on("SIGTERM", () => { for (const socket of sockets) socket.destroy(); server.close(() => process.exit(0)); });
