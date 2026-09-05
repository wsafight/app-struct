FROM mcr.microsoft.com/playwright:v1.62.1-noble
USER root
RUN apt-get update && apt-get install --no-install-recommends -y fonts-noto-cjk fonts-noto-core \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -g 10001 appstruct && useradd -u 10001 -g 10001 -m appstruct \
    && mkdir -p /run/appstruct-renderer && chown 10001:10001 /run/appstruct-renderer
WORKDIR /opt/renderer
COPY package.json pnpm-lock.yaml ./
RUN npm install --global pnpm@11.25.0 && pnpm install --frozen-lockfile --ignore-scripts
COPY server.mjs render.mjs resources.mjs test.mjs NOTICE LICENSE-SECCOMP ./
USER 10001:10001
ENV APPSTRUCT_RENDERER_SOCKET=/run/appstruct-renderer/renderer.sock
HEALTHCHECK --interval=10s --timeout=3s CMD node --input-type=module -e 'import net from "node:net"; const s=net.connect(process.env.APPSTRUCT_RENDERER_SOCKET); s.on("connect",()=>{s.end();process.exit(0)});s.on("error",()=>process.exit(1));setTimeout(()=>process.exit(1),2000)'
ENTRYPOINT ["node", "--max-old-space-size=128", "/opt/renderer/server.mjs"]
