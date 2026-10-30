"""Radiance Music's vocal splitter (runs on the build server, CPU only).

    split.py IN OUT_DIR [--from SECONDS] [--to SECONDS] [--model htdemucs|htdemucs_6s]
    split.py --join OUT_DIR A_DIR B_DIR CUT OVERLAP

A part of the song can be split (--from/--to), so two machines can share one
song; --join puts the two parts back together, crossfading across the stretch
both did (A: 0 .. CUT+OVERLAP, B: CUT-OVERLAP .. the end).

IN: FLAC, WAV, Ogg Opus or Ogg Vorbis. Writes OUT_DIR/vocals.EXT (the voice
alone), OUT_DIR/music.EXT (everything but the voice) and the other three parts
(drums, bass, other: for the player's mixer) at the model's 44.1 kHz,
using Demucs (htdemucs): FLAC at the input's bit depth when the input is
lossless, else Ogg Vorbis at high quality (a compressed song gains nothing from
a lossless copy, and the files travel back over the network). The last line
printed is "files vocals.EXT music.EXT …". With --model htdemucs_6s (asked for
song by song), the beat's piano and guitar come out as parts of their own too.
"""

import os
import sys

import numpy as np
import soundfile as sf
import torch
from demucs.apply import apply_model
from demucs.pretrained import get_model


def read_compressed(src: str, rate: int = 48000) -> tuple[np.ndarray, int]:
    """Opus / Vorbis / anything ffmpeg reads, via PyAV (libsndfile's Ogg reader
    crashes on some files): float32 (samples, 2) at `rate`."""
    import av

    chunks = []
    with av.open(src) as f:
        resampler = av.AudioResampler(format="flt", layout="stereo", rate=rate)
        for frame in f.decode(audio=0):
            for out in resampler.resample(frame):
                chunks.append(out.to_ndarray().reshape(-1, 2))
        for out in resampler.resample(None):
            chunks.append(out.to_ndarray().reshape(-1, 2))
    return np.concatenate(chunks).astype(np.float32), rate


def write(path_no_ext: str, data: np.ndarray, rate: int, fmt: str, subtype: str, ext: str, extra: dict) -> str:
    data = np.ascontiguousarray(np.clip(data, -1.0, 1.0))
    tmp = path_no_ext + ".part"
    # in blocks: libsndfile's Ogg writer crashes on one huge write
    with sf.SoundFile(tmp, "w", samplerate=rate, channels=2, format=fmt, subtype=subtype, **extra) as f:
        for i in range(0, len(data), 16384):
            f.write(data[i : i + 16384])
    os.replace(tmp, f"{path_no_ext}.{ext}")
    return os.path.basename(f"{path_no_ext}.{ext}")


def join(out: str, a_dir: str, b_dir: str, cut: float, overlap: float) -> None:
    os.makedirs(out, exist_ok=True)
    names = []
    for name in sorted({f.split(".")[0] for f in os.listdir(a_dir) if f.endswith((".flac", ".ogg"))}):
        fa = next(os.path.join(a_dir, f) for f in os.listdir(a_dir) if f.startswith(name + ".") and f.endswith((".flac", ".ogg")))
        fb = next((os.path.join(b_dir, f) for f in os.listdir(b_dir) if f.startswith(name + ".") and f.endswith((".flac", ".ogg"))), None)
        if fb is None:
            continue
        ext = fa.rsplit(".", 1)[1]
        if ext == "flac":
            a, rate = sf.read(fa, dtype="float32", always_2d=True)
            b, _ = sf.read(fb, dtype="float32", always_2d=True)
            fmt, subtype, extra = "FLAC", sf.info(fa).subtype, {}
        else:
            a, rate = read_compressed(fa, 44100)
            b, _ = read_compressed(fb, 44100)
            fmt, subtype, extra = "OGG", "VORBIS", {"compression_level": 0.15}
        n = min(int(2 * overlap * rate), len(a), len(b))
        fade = np.linspace(0.0, 1.0, n, dtype=np.float32)[:, None]
        joined = np.concatenate([a[: len(a) - n], a[len(a) - n :] * (1 - fade) + b[:n] * fade, b[n:]])
        names.append(write(os.path.join(out, name), joined, rate, fmt, subtype, ext, extra))
    print(f"files {' '.join(names)}", flush=True)


def main() -> None:
    if sys.argv[1] == "--join":
        join(sys.argv[2], sys.argv[3], sys.argv[4], float(sys.argv[5]), float(sys.argv[6]))
        return
    src, out = sys.argv[1], sys.argv[2]
    opts = dict(zip(sys.argv[3::2], sys.argv[4::2]))
    start, end = float(opts.get("--from", 0)), opts.get("--to")
    torch.set_num_threads(int(os.environ.get("SPLIT_THREADS", os.cpu_count() or 4)))
    name = opts.get("--model", "htdemucs")
    if name not in ("htdemucs", "htdemucs_6s"):
        sys.exit(f"no such model: {name}")
    model = get_model(name)
    model.eval()
    try:
        info = sf.info(src)
        kind = info.subtype if info.format in ("FLAC", "WAV", "AIFF") else "COMPRESSED"
    except RuntimeError:
        kind = "COMPRESSED"
    if kind in ("PCM_24", "PCM_32", "FLOAT", "DOUBLE"):
        fmt, subtype, ext, extra = "FLAC", "PCM_24", "flac", {}
    elif kind in ("PCM_16", "PCM_S8", "PCM_U8"):
        fmt, subtype, ext, extra = "FLAC", "PCM_16", "flac", {}
    else:
        fmt, subtype, ext, extra = "OGG", "VORBIS", "ogg", {"compression_level": 0.15}
    if kind == "COMPRESSED":
        audio, rate = read_compressed(src)
    else:
        audio, rate = sf.read(src, dtype="float32", always_2d=True)
    # just a part of the song (two machines sharing one)
    audio = audio[int(start * rate) : int(float(end) * rate) if end else len(audio)]
    x = torch.from_numpy(audio.T.copy())
    if x.shape[0] == 1:
        x = x.repeat(2, 1)
    x = x[:2]
    if rate != model.samplerate:
        import julius

        x = julius.resample_frac(x, rate, model.samplerate)
    ref = x.mean(0)
    mean, std = ref.mean(), ref.std() + 1e-8
    print("progress 5", flush=True)
    # real progress: Demucs goes through the song in chunks; each one done is reported
    import demucs.apply as demucs_apply

    class Chunks:
        def __init__(self, items, **_):
            self.items = list(items)

        def __iter__(self):
            for i, item in enumerate(self.items):
                yield item
                print(f"progress {5 + int(93 * (i + 1) / max(1, len(self.items)))}", flush=True)

    demucs_apply.tqdm = type("tqdm", (), {"tqdm": Chunks})
    with torch.no_grad():
        stems = apply_model(model, ((x - mean) / std)[None], split=True, overlap=0.25, progress=True, num_workers=0)[0]
    stems = stems * std + mean
    voice = model.sources.index("vocals")
    vocals = stems[voice]
    music = stems.sum(0) - vocals
    os.makedirs(out, exist_ok=True)
    parts = [("vocals", vocals), ("music", music)] + [(n, stems[i]) for i, n in enumerate(model.sources) if n != "vocals"]
    names = [write(os.path.join(out, name), part.T.numpy(), model.samplerate, fmt, subtype, ext, extra) for name, part in parts]
    print("progress 100", flush=True)
    print(f"files {' '.join(names)}", flush=True)


if __name__ == "__main__":
    main()
