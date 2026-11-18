# NewsQueue headless server: engine + JSON API + web UI on port 8480.
#
#   docker build -t newsqueue -f Dockerfile .        (run in app/)
#   docker compose up -d                              (see compose.yaml)
#
# Settings, queue, history and logs live in /config; set the download folder
# to /downloads with POST /api/settings {"download_dir":"/downloads"} or by
# editing /config/newsqueue-settings.json. Servers are configured in that file
# (the API never shows or changes accounts).

FROM rust:1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release -p newsqueue-server

FROM debian:bookworm-slim
# unrar lives in non-free; 7zip covers 7z/ZIP; par2 verifies and repairs.
RUN sed -i 's/^Components: main$/Components: main non-free/' /etc/apt/sources.list.d/debian.sources \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 unrar 7zip par2 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 1000 newsqueue \
    && mkdir -p /config /downloads && chown newsqueue /config /downloads
COPY --from=build /src/target/release/newsqueue-server /usr/local/bin/newsqueue-server
# The egui web build is not included (it needs wasm-bindgen); / serves the
# plain HTML page instead. Mount a web/build-web.ps1 output and pass --web to use it.
USER newsqueue
ENV NQ_CONFIG=/config NQ_BIND=0.0.0.0:8480
VOLUME ["/config", "/downloads"]
EXPOSE 8480
# NQ_API_KEY must be set: the server refuses a non-loopback address without one.
CMD ["newsqueue-server", "--config", "/config"]
