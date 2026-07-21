# From-source image. Release CI uses Dockerfile.runtime with prebuilt binaries.

FROM oven/bun:1.3.14-debian AS client
WORKDIR /src
COPY logo ./logo
RUN mkdir -p zoeken/zoeken-server/assets \
    && cp logo/zoeken-logo.svg zoeken/zoeken-server/assets/zoeken-logo.svg

WORKDIR /src/zoeken-client
COPY zoeken-client/package.json zoeken-client/bun.lock ./
RUN bun install --frozen-lockfile
COPY zoeken-client/ ./
RUN bun run build \
    && cp -f /src/logo/zoeken-logo.svg /src/zoeken/zoeken-server/assets/zoeken-logo.svg \
    && printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
       '<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">' \
       '  <xsl:output method="html"/>' \
       '  <xsl:template match="/"><html><body><xsl:apply-templates/></body></html></xsl:template>' \
       '</xsl:stylesheet>' > /src/zoeken/zoeken-server/assets/rss.xsl

FROM rust:1-bookworm AS chef
WORKDIR /src
RUN cargo install cargo-chef --locked --version 0.1.77

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS backend
WORKDIR /src
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake clang libclang-dev \
    && rm -rf /var/lib/apt/lists/*
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json --bin zoeken-server
COPY . .
COPY --from=client /src/zoeken/zoeken-server/assets ./zoeken/zoeken-server/assets
RUN cargo build --release --bin zoeken-server --locked

FROM debian:bookworm-slim AS runtime

ARG VERSION=1.1.0
ARG REVISION=unknown
ARG CREATED=unknown

LABEL org.opencontainers.image.title="Zoeken" \
      org.opencontainers.image.description="SearXNG-compatible metasearch engine by Greenstorm" \
      org.opencontainers.image.url="https://github.com/Greenstorm5417/zoeken" \
      org.opencontainers.image.source="https://github.com/Greenstorm5417/zoeken" \
      org.opencontainers.image.documentation="https://github.com/Greenstorm5417/zoeken#readme" \
      org.opencontainers.image.licenses="AGPL-3.0-or-later" \
      org.opencontainers.image.authors="Greenstorm <sdussinger1007@gmail.com>" \
      org.opencontainers.image.vendor="Greenstorm" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${CREATED}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /app zoeken \
    && mkdir -p /usr/share/licenses/zoeken /usr/share/doc/zoeken /usr/share/zoeken/plugins /etc/zoeken /var/lib/zoeken \
    && chown zoeken:zoeken /var/lib/zoeken
WORKDIR /app

COPY --from=backend /src/target/release/zoeken-server /app/zoeken-server
COPY --from=client /src/zoeken/zoeken-server/assets /app/assets
COPY --from=backend /src/zoeken/zoeken-plugins/plugins /usr/share/zoeken/plugins
COPY packaging/debian/zoeken.settings.yml /etc/zoeken/settings.yml
COPY packaging/debian/limiter.toml /etc/zoeken/limiter.toml
COPY default.config.yml /usr/share/doc/zoeken/default.config.yml
COPY LICENSE /usr/share/licenses/zoeken/LICENSE
COPY LICENSE /app/LICENSE

ENV APP_ASSETS_DIR=/app/assets \
    APP_SETTINGS_PATH=/etc/zoeken/settings.yml

USER zoeken

EXPOSE 8888

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8888/healthz || exit 1

ENTRYPOINT ["/app/zoeken-server"]
