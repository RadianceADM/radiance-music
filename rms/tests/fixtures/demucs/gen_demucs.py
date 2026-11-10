"""Makes the Rust splitter's blending reference (rms/src/demucs.rs tests) from Demucs's own apply_model, the way
server/split.py calls it (split=True, overlap=0.25; shifts=0, as Python's random shift can't be matched).
Run once, on a build machine with torch + demucs (never shipped, never run by the server):

    python gen_demucs.py OUT_DIR

The stand-in model answers each part as its piece times a ramp along the piece (and the part's number), so where the
pieces start, how a short one is padded and trimmed, and how they're weighted all show in the result. Pieces of 1000
frames (sample rate 1000, segment 1 s) keep it small.
"""
import json, os, sys

import numpy as np
import torch
from demucs.apply import apply_model

out_dir = sys.argv[1]
N = 1000


class Ramp(torch.nn.Module):
    samplerate = 1000
    segment = 1.0
    sources = ["drums", "bass", "other", "vocals"]
    audio_channels = 2

    def valid_length(self, length):
        assert length <= N
        return N

    def forward(self, mix):  # [B, 2, T] → [B, 4, 2, T]
        t = mix.shape[-1]
        ramp = 1.0 + torch.arange(t, dtype=mix.dtype) / N
        return torch.stack([mix * ramp * (s + 1) for s in range(4)], dim=1)


rng = np.random.default_rng(11)
cases = []
for total in [700, 1000, 1001, 2600]:
    # 3 decimals: short in JSON, and the same float32 on both sides
    x = np.round(rng.standard_normal((2, total)) * 0.3 + 0.05, 3).astype(np.float32)
    t = torch.from_numpy(x)
    ref = t.mean(0)
    mean, std = ref.mean(), ref.std() + 1e-8  # as split.py: torch's std (n - 1)
    with torch.no_grad():
        out = apply_model(Ramp(), ((t - mean) / std)[None], shifts=0, split=True, overlap=0.25, progress=False, num_workers=0)[0]
    out = out * std + mean
    # interleaved, as the Rust side keeps sound
    cases.append({"total": total, "x": [round(float(v), 3) for v in x.T.flatten()], "parts": [[round(float(v), 6) for v in out[p].T.flatten()] for p in range(4)]})
json.dump({"n": N, "cases": cases}, open(os.path.join(out_dir, "apply_model.json"), "w"))
print("made", len(cases), "cases")
