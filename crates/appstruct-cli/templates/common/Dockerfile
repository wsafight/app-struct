# syntax=docker/dockerfile:1.7

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install --no-install-recommends --yes ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 appstruct

WORKDIR /app
COPY .appstruct/cache/backend-target/release/appstruct-generated-server /usr/local/bin/appstruct

RUN chmod 0755 /usr/local/bin/appstruct
USER appstruct

ENV APPSTRUCT_BIND=0.0.0.0:3000
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/appstruct"]
