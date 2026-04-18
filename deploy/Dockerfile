# syntax=docker/dockerfile:1.7

ARG NODE_IMAGE=node:20-alpine
ARG RUST_IMAGE=rust:1.88-alpine3.21
ARG RUNTIME_IMAGE=alpine:3.19
ARG CARGO_BUILD_JOBS

FROM ${NODE_IMAGE} AS frontend-builder
WORKDIR /app
ENV npm_config_cache=/root/.npm

COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm \
    npm ci --prefer-offline --no-audit

COPY index.html svelte.config.js tsconfig.json vite.config.ts ./
COPY public ./public
COPY src ./src

RUN --mount=type=cache,target=/root/.npm \
    npm run build

FROM ${RUST_IMAGE} AS backend-builder
ARG CARGO_BUILD_JOBS
WORKDIR /app

RUN apk add --no-cache build-base musl-dev
ENV CARGO_NET_RETRY=8 \
    CARGO_HTTP_TIMEOUT=600 \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

COPY src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/build.rs ./src-tauri/
RUN mkdir -p /app/src-tauri/src/bin \
    && printf 'pub fn docker_build_cache_warmup() {}\n' > /app/src-tauri/src/lib.rs \
    && printf 'fn main() {}\n' > /app/src-tauri/src/main.rs \
    && printf 'fn main() {}\n' > /app/src-tauri/src/bin/acliv-web.rs
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    sh -ec 'cargo build --locked --release ${CARGO_BUILD_JOBS:+--jobs="$CARGO_BUILD_JOBS"} --manifest-path src-tauri/Cargo.toml --no-default-features --features web --bin acliv-web'

COPY src-tauri ./src-tauri
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    sh -ec 'cargo build --locked --release ${CARGO_BUILD_JOBS:+--jobs="$CARGO_BUILD_JOBS"} --manifest-path src-tauri/Cargo.toml --no-default-features --features web --bin acliv-web'

FROM ${RUNTIME_IMAGE}
WORKDIR /app

RUN apk add --no-cache ca-certificates curl su-exec \
    && addgroup -g 1000 -S app \
    && adduser -u 1000 -S -G app app

COPY --from=frontend-builder --chown=app:app /app/dist /app/dist
COPY --from=backend-builder --chown=app:app /app/src-tauri/target/release/acliv-web /app/acliv-web
COPY --chown=app:app deploy/docker-entrypoint.sh /app/docker-entrypoint.sh

RUN chmod +x /app/docker-entrypoint.sh \
    && mkdir -p /app/data

ENV ACLIV_HOST=0.0.0.0
ENV ACLIV_PORT=17860
ENV ACLIV_HOME=/app/data
ENV ACLIV_CLAUDE_DIR=/host-data/claude/projects
ENV ACLIV_CODEX_DIR=/host-data/codex/sessions
ENV ACLIV_GEMINI_DIR=/host-data/gemini/tmp
ENV ACLIV_OPENCLAW_DIR=/host-data/openclaw/agents
ENV ACLIV_OPENCODE_DIR=/host-data/opencode/storage
ENV ACLIV_RUN_AS_ROOT=0

EXPOSE 17860

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${ACLIV_PORT:-17860}/api/health" >/dev/null || exit 1

ENTRYPOINT ["/app/docker-entrypoint.sh"]
CMD ["/app/acliv-web"]
