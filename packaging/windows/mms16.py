# The MMS word-timing model stored as fp16 (half the size) gives the same frames as fp32?
# Converts model.pt, then runs both on 60 s of a real vocal part and compares.
import sys, torch, torchaudio, soundfile as sf
src, dst, vocals = sys.argv[1], sys.argv[2], sys.argv[3]
sd = torch.load(src, map_location="cpu", weights_only=True)
torch.save({k: (v.half() if v.is_floating_point() else v) for k, v in sd.items()}, dst)
bundle = torchaudio.pipelines.MMS_FA
def model_from(path):
    st = torch.load(path, map_location="cpu", weights_only=True)
    params = dict(bundle._params); params["aux_num_out"] = st["aux.weight"].shape[0]
    m = torchaudio.models.wav2vec2_model(**params)
    m.load_state_dict({k: v.float() for k, v in st.items()})
    return m.eval()
a, b = model_from(src), model_from(dst)
x, sr = sf.read(vocals, dtype="float32", always_2d=True)
x = torch.from_numpy(x.mean(1))[None]
x = torchaudio.functional.resample(x, sr, bundle.sample_rate)[:, : 60 * bundle.sample_rate]
with torch.inference_mode():
    ea, _ = a(x); eb, _ = b(x)
agree = (ea.argmax(-1) == eb.argmax(-1)).float().mean().item()
print(f"frames {ea.shape[1]}, argmax agree {agree*100:.3f} %, max |diff| {(ea-eb).abs().max().item():.4f}, mean |diff| {(ea-eb).abs().mean().item():.5f}")
import os; print(f"sizes: {os.path.getsize(src)/1e6:.0f} MB -> {os.path.getsize(dst)/1e6:.0f} MB")
