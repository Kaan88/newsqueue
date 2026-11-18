# NewsQueue headless server: engine, JSON API and the egui web UI on 8480.
#
#   docker build -t newsqueue .            (run in app/)
#   docker compose up -d                   (see compose.yaml)
#
# Volumes: /config (settings, queue, history, stats, logs), /downloads
# (finished jobs), /incomplete (temporary files), /watch (NZB files dropped
# here are queued). A first start points the settings at these folders.
# Servers, folders, PAR2/unpack, scheduler and RSS are set in the web UI
# (Settings); passwords are never sent back to the browser.
#
# Environment: NQ_API_KEY (required), PUID/PGID (default 1000), TZ.
# No TLS: put it behind a reverse proxy if it is reachable from outside.

FROM rust:1-trixie AS build
RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version 0.2.130 --locked
WORKDIR /src
COPY . .
RUN cargo build --release -p newsqueue-server \
    && cargo build --release -p newsqueue-web --target wasm32-unknown-unknown \
    && mkdir -p /out/web/pkg \
    && wasm-bindgen --target web --no-typescript --out-dir /out/web/pkg \
        target/wasm32-unknown-unknown/release/newsqueue-web.wasm \
    && cp web/index.html /out/web/ \
    && cp target/release/newsqueue-server /out/

FROM debian:trixie-slim
# unrar lives in non-free; 7zip covers 7z/ZIP; par2 verifies and repairs.
RUN sed -i 's/^Components: main$/Components: main non-free/' /etc/apt/sources.list.d/debian.sources \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3t64 tzdata unrar 7zip par2 \
    && rm -rf /var/lib/apt/lists/*
# The server finds the web UI in `web` beside its executable.
COPY --from=build /out/ /opt/newsqueue/
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 0755 /usr/local/bin/docker-entrypoint.sh \
    && ln -s /opt/newsqueue/newsqueue-server /usr/local/bin/newsqueue-server
ENV NQ_CONFIG=/config NQ_BIND=0.0.0.0:8480 PUID=1000 PGID=1000
VOLUME ["/config", "/downloads", "/incomplete", "/watch"]
EXPOSE 8480
HEALTHCHECK --interval=30s --timeout=6s --start-period=10s --retries=3 \
    CMD ["newsqueue-server", "--health-check"]
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
# NQ_API_KEY must be set: the server refuses a non-loopback address without one.
CMD ["newsqueue-server", "--config", "/config"]
