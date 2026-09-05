# syntax=docker/dockerfile:1.7

FROM rust:1.98.0-bookworm AS backend-build
WORKDIR /build
COPY generated/backend ./generated/backend
COPY generated/server ./generated/server
COPY app ./app
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked --manifest-path generated/server/Cargo.toml

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install --no-install-recommends --yes ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 appstruct

WORKDIR /app
COPY --from=backend-build /build/generated/server/target/release/appstruct-generated-server /usr/local/bin/appstruct

RUN chmod 0755 /usr/local/bin/appstruct \
    && mkdir -p /app/.appstruct/files \
    && chown -R appstruct:appstruct /app/.appstruct
USER appstruct

ENV APPSTRUCT_BIND=0.0.0.0:3000
EXPOSE 3000
HEALTHCHECK --interval=10s --timeout=3s --start-period=20s --retries=5 \
    CMD curl --fail --silent --max-time 2 http://127.0.0.1:3000/health/ready || exit 1
ENTRYPOINT ["/usr/local/bin/appstruct"]
