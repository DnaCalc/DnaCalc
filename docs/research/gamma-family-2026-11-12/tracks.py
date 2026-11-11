import numpy as np, sys
spec = np.load(sys.argv[1] if len(sys.argv) > 1 else "e5_spec.npy"); W = spec.shape[0]; fr = np.fft.rfftfreq(1024)
med = np.median(spec[:, 1:], axis=1)
thr = float(sys.argv[2]) if len(sys.argv) > 2 else 25
peaks = []
for w in range(W):
    s = spec[w]; pk = []
    for k in range(2, 511):
        if s[k] > thr * med[w] and s[k] >= s[k - 1] and s[k] >= s[k + 1]:
            # parabolic interpolation on log power
            a, b, c = np.log(s[k - 1]), np.log(s[k]), np.log(s[k + 1])
            off = 0.5 * (a - c) / (a - 2 * b + c) if (a - 2 * b + c) != 0 else 0
            pk.append(((k + off) / 1024, s[k] / med[w]))
    peaks.append(pk)
# link peaks into tracks: a track continues if a peak in the next window lies within 0.006 of its extrapolation
tracks = []
for w in range(W):
    used = set()
    for t in tracks:
        if t["end"] != w - 1: continue
        last = t["f"][-1]; slope = (t["f"][-1] - t["f"][-2]) if len(t["f"]) > 1 else 0
        pred = last + slope
        best = None
        for i, (f, p) in enumerate(peaks[w]):
            if i in used: continue
            if abs(f - pred) < 0.006 and (best is None or abs(f - pred) < abs(peaks[w][best][0] - pred)): best = i
        if best is not None:
            used.add(best); t["f"].append(peaks[w][best][0]); t["p"].append(peaks[w][best][1]); t["end"] = w
    for i, (f, p) in enumerate(peaks[w]):
        if i not in used: tracks.append({"start": w, "end": w, "f": [f], "p": [p]})
tracks = [t for t in tracks if t["end"] - t["start"] >= 12]
tracks.sort(key=lambda t: -sum(t["p"]))
for t in tracks[:25]:
    ys = [(w + 0.5) / 256 for w in range(t["start"], t["end"] + 1)]
    f = t["f"]
    print("y %.3f..%.3f n=%3d  f: %s  meanpow %.0f" % (ys[0], ys[-1], len(f),
          " ".join("%.4f" % f[i] for i in range(0, len(f), max(1, len(f) // 8))), np.mean(t["p"])))
