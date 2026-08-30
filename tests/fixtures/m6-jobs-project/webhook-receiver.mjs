import { createHmac, timingSafeEqual } from "node:crypto";
import { appendFileSync } from "node:fs";
import { createServer } from "node:http";

const [port, capturePath, secret] = process.argv.slice(2);
if (!port || !capturePath || !secret) {
  throw new Error("usage: webhook-receiver.mjs <port> <capture-path> <secret>");
}

const server = createServer((request, response) => {
  const chunks = [];
  request.on("data", (chunk) => chunks.push(chunk));
  request.on("end", () => {
    if (request.url === "/hang") return;

    const body = Buffer.concat(chunks);
    const timestamp = request.headers["x-appstruct-timestamp"];
    const actual = request.headers["x-appstruct-signature"];
    const digest = createHmac("sha256", secret)
      .update(`${timestamp}.`)
      .update(body)
      .digest("hex");
    const expected = `v1=${digest}`;
    const signatureValid =
      typeof actual === "string" &&
      actual.length === expected.length &&
      timingSafeEqual(Buffer.from(actual), Buffer.from(expected));
    appendFileSync(
      capturePath,
      `${JSON.stringify({
        body: JSON.parse(body.toString("utf8")),
        delivery: request.headers["x-appstruct-delivery"],
        event: request.headers["x-appstruct-event"],
        timestamp,
        signatureValid,
      })}\n`,
    );
    response.writeHead(signatureValid ? 204 : 401).end();
  });
});

server.listen(Number(port), "127.0.0.1", () => {
  process.stdout.write("ready\n");
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    server.closeAllConnections();
    server.close(() => process.exit(0));
  });
}
