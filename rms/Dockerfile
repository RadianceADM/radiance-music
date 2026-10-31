# Radiance Music Server (RMS): the image that Hostinger's "Deploy on Hostinger"
# button (and `docker compose up -d` anywhere) runs.
#
# Built from the repository's root: it needs the server (rms/) and the app's
# own splitter scripts (server/split.py and server/align.py):
#
#   docker build -f rms/Dockerfile -t ghcr.io/radianceadm/radiance-music-server:latest .
#
# The splitting models are NOT in the image (owner, 2026-10-31: "download on
# first open"): the server downloads them into its data volume at its first
# start (about 1.3 GB, once), with the progress on its web page.

# 1. the server itself
FROM docker.io/library/rust:1-slim-trixie AS build
WORKDIR /src
COPY rms/ ./
RUN CARGO_PROFILE_RELEASE_STRIP=true cargo build --release --locked

# 2. what runs: Debian, Python with the splitter (the app's own pip list, as
#    setup.rs install() has it: PyTorch for the CPU, torchaudio, Demucs,
#    soundfile, PyAV) and ffmpeg
FROM docker.io/library/debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends python3 python3-venv ffmpeg ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*
RUN python3 -m venv /opt/rms/venv \
 && /opt/rms/venv/bin/pip install --no-cache-dir -q --upgrade pip \
 && /opt/rms/venv/bin/pip install --no-cache-dir --progress-bar off --only-binary=:all: \
      --index-url https://download.pytorch.org/whl/cpu --extra-index-url https://pypi.org/simple \
      torch torchaudio soundfile av numpy pyyaml \
 && /opt/rms/venv/bin/pip install --no-cache-dir -q --only-binary=:all: demucs \
 && /opt/rms/venv/bin/python -c "import torch, torchaudio, demucs, soundfile, av; assert hasattr(torchaudio.pipelines, 'MMS_FA'); print('splitter ok: torch', torch.__version__, 'torchaudio', torchaudio.__version__)"
COPY server/split.py server/align.py /opt/rms/scripts/
COPY --from=build /src/target/release/rms /usr/local/bin/rms
# its own user; the data volume is its home (the library, server.db, the models)
RUN useradd --uid 10001 --no-create-home --home-dir /data --shell /usr/sbin/nologin rms \
 && mkdir -p /data && chown rms:rms /data
USER rms
ENV RMS_DATA=/data \
    RMS_BIND=0.0.0.0:8080 \
    RMS_PYTHON=/opt/rms/venv/bin/python \
    RMS_SCRIPTS=/opt/rms/scripts \
    HOME=/data
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/rms"]
