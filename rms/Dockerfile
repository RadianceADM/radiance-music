# Radiance Music Server (RMS): the image that Hostinger's "Deploy on Hostinger"
# button (and `docker compose up -d` anywhere) runs.
#
# Built from the repository's root (its context is the repository):
#
#   docker build -f rms/Dockerfile -t ghcr.io/radianceadm/radiance-music-server:latest .
#
# The server splits songs and times their words itself, in Rust, with no
# Python (owner, 2026-11-09, #72). What it runs on is NOT in the image (owner,
# 2026-10-31: "download on first open"): ONNX Runtime and the two models
# (Demucs, MMS) download into its data volume at its first start (about
# 800 MB, once), with the progress on its web page.

# 1. the server itself
FROM docker.io/library/rust:1-slim-trixie AS build
WORKDIR /src
COPY rms/ ./
RUN CARGO_PROFILE_RELEASE_STRIP=true cargo build --release --locked

# 2. what runs: Debian, ffmpeg (songs in, parts out) and the server
FROM docker.io/library/debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/rms /usr/local/bin/rms
# its own user; the data volume is its home (the library, server.db, the models)
RUN useradd --uid 10001 --no-create-home --home-dir /data --shell /usr/sbin/nologin rms \
 && mkdir -p /data && chown rms:rms /data
USER rms
ENV RMS_DATA=/data \
    RMS_BIND=0.0.0.0:8080 \
    HOME=/data
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/rms"]
