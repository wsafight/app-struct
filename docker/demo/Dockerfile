# syntax=docker/dockerfile:1.7

FROM rust:1-bookworm AS rust-builder
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY examples/saas-demo ./examples/saas-demo
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/workspace/target \
    cargo build --release --locked -p appstruct-cli \
    && cp /workspace/target/release/appstruct /tmp/appstruct
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/workspace/examples/saas-demo/target \
    CARGO_TARGET_DIR=/workspace/examples/saas-demo/target \
    cargo build --release --manifest-path examples/saas-demo/generated/server/Cargo.toml \
    && cp /workspace/examples/saas-demo/target/release/appstruct-generated-server /tmp/appstruct-generated-server

FROM node:22-bookworm-slim AS web-builder
WORKDIR /web
RUN corepack enable
COPY examples/saas-demo/generated/web ./generated/web
COPY examples/saas-demo/app/web ./app/web
WORKDIR /web/generated/web
RUN pnpm install --frozen-lockfile && pnpm run build

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --no-install-recommends --yes ca-certificates curl nginx \
    && rm -rf /var/lib/apt/lists/* \
    && rm -f /etc/nginx/sites-enabled/default

WORKDIR /demo
COPY --from=rust-builder /tmp/appstruct /usr/local/bin/appstruct
COPY --from=rust-builder /tmp/appstruct-generated-server /usr/local/bin/appstruct-generated-server
COPY --from=web-builder /web/generated/web/dist /usr/share/nginx/html
COPY examples/saas-demo/appstruct.yaml ./appstruct.yaml
COPY examples/saas-demo/appstruct.lock ./appstruct.lock
COPY examples/saas-demo/spec ./spec
COPY docker/demo/nginx.conf /etc/nginx/conf.d/default.conf
COPY docker/demo/entrypoint.sh /usr/local/bin/demo-entrypoint.sh

RUN chmod 0755 /usr/local/bin/appstruct /usr/local/bin/appstruct-generated-server /usr/local/bin/demo-entrypoint.sh \
    && mkdir -p /demo/.appstruct/files

ENV APPSTRUCT_BIND=127.0.0.1:3000 \
    APPSTRUCT_ENV=development \
    APPSTRUCT_AUTH_MAIL_MODE=capture \
    APPSTRUCT_FILE_ROOT=/demo/.appstruct/files \
    APPSTRUCT_ALLOWED_ORIGIN=http://localhost:8080

EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --start-period=25s --retries=6 \
    CMD curl --fail --silent --max-time 2 http://127.0.0.1:8080/health/ready || exit 1
ENTRYPOINT ["/usr/local/bin/demo-entrypoint.sh"]
