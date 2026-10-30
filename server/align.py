"""Word timing for a song's lyrics from its Vocals part, with Meta's MMS forced
aligner (torchaudio's MMS_FA: one model for 1100+ languages, Tagalog included),
on the processor. Radiance Music runs it on the split server after a split.

    align.py VOCALS [OUT.yaml] < lyrics.json

lyrics.json is {"lines": [{"start": s, "end": e, "text": "..."}, ...]} (timed by
line: each line's words are placed within its own stretch, a little wider) or
{"plain": "..."} (no timing: the whole song at once, a line per text line).
Writes a Lyricsfile (YAML) with each line's and each word's start and end to
OUT and to stdout. "progress N" (0-100) lines go to stderr.

Measured on JamendoLyrics (hand-marked word starts, Demucs vocals): with line
timing 92 % of words within 0.3 s (median 0.04 s); plain text 87 %.
"""

import json
import os
import re
import sys
import unicodedata

import soundfile as sf
import torch
import torchaudio

torch.set_num_threads(int(os.environ.get("ALIGN_THREADS", "6")))
WINDOW, CONTEXT = 30.0, 2.0  # seconds a pass, and the sound kept either side of it


def progress(p):
    print(f"progress {int(p)}", file=sys.stderr, flush=True)


def norm(word):
    """The letters the model knows: lowercase a-z and ', accents off."""
    w = unicodedata.normalize("NFKD", word.lower())
    w = "".join(c for c in w if not unicodedata.combining(c))
    return re.sub(r"[^a-z']", "", w)


def main():
    vocals, out = sys.argv[1], (sys.argv[2] if len(sys.argv) > 2 else None)
    given = json.load(sys.stdin)
    bundle = torchaudio.pipelines.MMS_FA
    model = bundle.get_model(with_star=False).eval()
    tokenize, align, rate = bundle.get_tokenizer(), bundle.get_aligner(), bundle.sample_rate
    progress(5)

    audio, sr = sf.read(vocals, dtype="float32", always_2d=True)
    wav = torchaudio.functional.resample(torch.from_numpy(audio.mean(1))[None], sr, rate)
    total = wav.size(1) / rate

    # the model's view of the whole song, 30 s at a time (its attention grows with the square)
    pieces, at = [], 0.0
    with torch.inference_mode():
        while at < total:
            a, b = max(0.0, at - CONTEXT), min(total, at + WINDOW + CONTEXT)
            em, _ = model(wav[:, int(a * rate):int(b * rate)])
            em = em[0]
            per = (b - a) / em.size(0)
            keep_from = int(round((at - a) / per))
            keep_to = int(round((min(total, at + WINDOW) - a) / per))
            pieces.append(em[keep_from:keep_to])
            at += WINDOW
            progress(5 + 75 * min(1.0, at / total))
    emission = torch.cat(pieces)
    spf = total / emission.size(0)  # seconds a frame

    def place(words, lo, hi):
        """Each word's (start, end) in seconds, aligned within lo..hi; None if it can't be."""
        a, b = max(0, int(lo / spf)), min(emission.size(0), int(hi / spf) + 1)
        keep = [(i, norm(w)) for i, w in enumerate(words)]
        keep = [(i, w) for i, w in keep if w]
        times = [None] * len(words)
        if not keep or b - a < 2:
            return times
        try:
            spans = align(emission[a:b], tokenize([w for _, w in keep]))
        except Exception:
            return times
        for (i, _), sp in zip(keep, spans):
            times[i] = ((a + sp[0].start) * spf, (a + sp[-1].end) * spf)
        return times

    if "lines" in given:
        lines = [l for l in given["lines"] if l.get("text", "").strip()]
        starts = [float(l["start"]) for l in lines]
        texts = [l["text"] for l in lines]
        groups = []
        for i, text in enumerate(texts):
            lo = max(0.0, starts[i] - 0.5)
            hi = (starts[i + 1] if i + 1 < len(lines) else float(lines[i].get("end", starts[i] + 6))) + 0.5
            groups.append((text, place(text.split(), lo, hi)))
    else:
        texts = [t for t in given.get("plain", "").splitlines() if t.strip() and not re.fullmatch(r"\s*\[.*\]\s*", t)]
        words = [w for t in texts for w in t.split()]
        times = place(words, 0.0, total)
        groups, k = [], 0
        for text in texts:
            n = len(text.split())
            groups.append((text, times[k:k + n]))
            k += n
    progress(95)

    # words with nothing to sing (numbers, signs) take the gap before the next
    y = ['version: "1.0"', "# word times: MMS forced alignment (Meta, torchaudio MMS_FA) on the song's Vocals part"]
    y.append("lines:")
    q = lambda s: json.dumps(s, ensure_ascii=False)
    ms = lambda s: int(round(s * 1000))
    last_end = 0.0
    for text, times in groups:
        words = text.split()
        known = [t for t in times if t]
        if not known:
            continue
        filled = []
        for i, t in enumerate(times):
            if t is None:
                nxt = next((x[0] for x in times[i + 1:] if x), None)
                s = filled[-1][1] if filled else (nxt if nxt is not None else known[0][0])
                t = (s, s)
            filled.append(t)
        start, end = filled[0][0], max(e for _, e in filled)
        y += [f"  - text: {q(text)}", f"    start_ms: {ms(start)}", f"    end_ms: {ms(max(end, start + 0.2))}", "    words:"]
        for i, (w, (s, e)) in enumerate(zip(words, filled)):
            tail = " " if i + 1 < len(words) else ""
            y += [f"      - text: {q(w + tail)}", f"        start_ms: {ms(s)}", f"        end_ms: {ms(max(e, s))}"]
        last_end = end
    text = "\n".join(y) + "\n"
    if out:
        tmp = out + ".part"
        with open(tmp, "w") as f:
            f.write(text)
        os.replace(tmp, out)
    sys.stdout.write(text)
    progress(100)


if __name__ == "__main__":
    main()
