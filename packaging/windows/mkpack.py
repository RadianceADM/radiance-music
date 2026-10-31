# The Windows splitter pack: python\ and models\ (pack.sh's splitter-pack folder), as one zip
# (deflate), downloaded by the app at its first start. Prints its size and SHA-256.
import hashlib, os, sys, zipfile
dist, out = sys.argv[1], sys.argv[2]
tmp = out + ".tmp"
with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for top in ("python", "models"):
        for root, dirs, files in os.walk(os.path.join(dist, top)):
            dirs.sort()
            for f in sorted(files):
                p = os.path.join(root, f)
                z.write(p, os.path.relpath(p, dist).replace(os.sep, "/"))
os.replace(tmp, out)
h = hashlib.sha256()
with open(out, "rb") as f:
    for b in iter(lambda: f.read(1 << 20), b""):
        h.update(b)
print(os.path.getsize(out), h.hexdigest())
