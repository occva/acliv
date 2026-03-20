ARG NODE_IMAGE=node:20-alpine
ARG RUST_IMAGE=rust:1.77-alpine3.19
ARG RUNTIME_IMAGE=alpine:3.19

FROM ${NODE_IMAGE} AS frontend-builder
WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci

COPY index.html svelte.config.js tsconfig.json vite.config.ts ./
COPY public ./public
COPY src ./src

RUN npm run build

FROM ${RUST_IMAGE} AS backend-builder
WORKDIR /app

RUN apk add --no-cache build-base musl-dev

COPY src-tauri ./src-tauri
RUN cargo build --release --manifest-path src-tauri/Cargo.toml --no-default-features --features web --bin aichv-web

FROM ${RUNTIME_IMAGE}
WORKDIR /app

RUN apk add --no-cache ca-certificates curl su-exec \
    && addgroup -g 1000 -S app \
    && adduser -u 1000 -S -G app app

COPY --from=frontend-builder --chown=app:app /app/dist /app/dist
COPY --from=backend-builder --chown=app:app /app/src-tauri/target/release/aichv-web /app/aichv-web
COPY --chown=app:app deploy/docker-entrypoint.sh /app/docker-entrypoint.sh

RUN chmod +x /app/docker-entrypoint.sh \
    && mkdir -p /app/data

ENV AICHV_HOST=0.0.0.0
ENV AICHV_PORT=17860
ENV AICHV_HOME=/host-home

EXPOSE 17860

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${AICHV_PORT:-17860}/api/health" >/dev/null || exit 1

ENTRYPOINT ["/app/docker-entrypoint.sh"]
CMD ["/app/aichv-web"]
