"""Makes the Rust word timing's references (rms/src/mms.rs tests) from torchaudio and server/align.py itself.
Run once, on a build machine with torch + torchaudio (never shipped, never run by the server):

    python gen_align.py OUT_DIR path/to/server/align.py

Scores are multiples of 1/4 (exact in float32 and short in JSON), so ties happen and the
port's tie-breaking is checked too.
"""
import contextlib, importlib.util, io, json, os, sys

import numpy as np
import soundfile as sf
import torch
import torchaudio
import torchaudio.functional as F

out_dir, align_py = sys.argv[1], sys.argv[2]
rng = np.random.default_rng(7)
LETTERS = torchaudio.pipelines.MMS_FA.get_dict(star=None)
assert len(LETTERS) == 28, LETTERS


def scores(frames):
    return (rng.integers(-40, 1, size=(frames, 28)) / 4.0).astype(np.float32)


# 1. forced alignment: the path, and the spans per word
cases = []
for frames, words in [(60, ["hello", "there"]), (40, ["aa", "bb"]), (12, ["hello"]), (11, ["ab", "ba", "a"]), (6, ["hello"]), (90, ["can't", "stop", "ikaw", "oh"]), (3, ["a"])]:
    em = scores(frames)
    tokens = [[LETTERS[c] for c in w] for w in words]
    flat = [t for w in tokens for t in w]
    case = {"frames": frames, "emission": em.flatten().tolist(), "words": tokens}
    try:
        path, _ = F.forced_align(torch.from_numpy(em)[None], torch.tensor([flat], dtype=torch.int32), blank=0)
        case["path"] = path[0].tolist()
        spans = torchaudio.pipelines.MMS_FA.get_aligner()(torch.from_numpy(em), tokens)
        case["spans"] = [[[s.token, s.start, s.end] for s in w] for w in spans]
    except Exception as e:  # too few frames for its letters
        case["path"] = None
        case["error"] = str(e).splitlines()[0]
    cases.append(case)
json.dump({"cases": cases}, open(os.path.join(out_dir, "forced_align.json"), "w"))

# 2. torchaudio's resampling (align.py takes the Vocals part to 16 kHz with it)
res = []
for rate, n in [(44_100, 4_410), (48_000, 3_000), (16_000, 500)]:
    x = (rng.standard_normal(n) * 0.3).astype(np.float32)
    y = F.resample(torch.from_numpy(x)[None], rate, 16_000)[0]
    res.append({"rate": rate, "x": x.tolist(), "y": y.tolist()})
json.dump({"cases": res}, open(os.path.join(out_dir, "resample.json"), "w"))

# 3. align.py itself, its model swapped for fixed scores (one pass: songs under 30 s), its own placing and YAML
spec = importlib.util.spec_from_file_location("align", align_py)
align = importlib.util.module_from_spec(spec)
spec.loader.exec_module(align)
real = torchaudio.pipelines.MMS_FA


class Model:
    def __init__(self, em):
        self.em = em

    def eval(self):
        return self

    def __call__(self, wav):
        return self.em[None], None


class Bundle:
    sample_rate = 16_000

    def __init__(self, em):
        self.em = em

    def get_model(self, with_star=False):
        return Model(self.em)

    def get_tokenizer(self):
        return real.get_tokenizer()

    def get_aligner(self):
        return real.get_aligner()


runs = []
givens = [
    ("lines", 10.0, {"lines": [{"start": 0.5, "end": 3.0, "text": "Hello there"}, {"start": 3.2, "text": "can't stop 123 Ikáw"}, {"start": 7.0, "end": 9.0, "text": "oh oh oh"}]}),
    ("too short", 4.0, {"lines": [{"start": 1.0, "text": "a"}, {"start": 1.01, "text": "supercalifragilistic expialidocious"}, {"start": 1.02, "end": 1.03, "text": "x"}, {"start": 2.0, "end": 3.5, "text": "la la"}]}),
    ("plain", 8.0, {"plain": "First line here\n[Chorus]\nsecond line\n\n  third été\r\n\"quoted\" \\ back"}),
    ("no letters", 5.0, {"lines": [{"start": 0.0, "end": 2.0, "text": "1 2 3"}, {"start": 2.0, "end": 4.0, "text": "sing it"}]}),
    ("blank lines", 6.0, {"lines": [{"start": 0.0, "text": "  "}, {"start": 1.0, "end": 5.5, "text": "Ooh, ooh-ooh"}]}),
]
for name, secs, given in givens:
    n = int(secs * 16_000)
    em = torch.from_numpy(scores(n // 320))
    wav = os.path.join(out_dir, "_silence.wav")
    sf.write(wav, np.zeros(n, dtype=np.float32), 16_000, subtype="FLOAT")
    torchaudio.pipelines.MMS_FA = Bundle(em)
    sys.argv, sys.stdin = ["align.py", wav], io.StringIO(json.dumps(given))
    out = io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
        align.main()
    torchaudio.pipelines.MMS_FA = real
    os.remove(wav)
    runs.append({"name": name, "samples": n, "frames": em.size(0), "emission": em.flatten().tolist(), "given": given, "yaml": out.getvalue()})
json.dump({"runs": runs}, open(os.path.join(out_dir, "align_py.json"), "w"), ensure_ascii=False)
print("made", len(cases), "alignments,", len(res), "resamplings,", len(runs), "align.py runs")
