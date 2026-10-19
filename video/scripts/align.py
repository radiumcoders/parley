"""Word timings for every clip from Gradium STT: what it heard (with times), and the
script's words (fillers included) aligned onto those times, unmatched ones interpolated.
Adds `heard` and `script` to src/data/timings.json."""
import difflib, json, pathlib, re, subprocess, sys, urllib.parse, urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
ENV = pathlib.Path.home() / ".config/parley/env"
KEY = next(l.split("=", 1)[1].strip() for l in ENV.read_text().splitlines() if l.startswith("GRADIUM_API_KEY="))
lines = json.loads((ROOT / "scripts/lines.json").read_text())
path = ROOT / "src/data/timings.json"
timings = json.loads(path.read_text())
only = set(sys.argv[1:])

def norm(w):
    return re.sub(r"[^\w']", "", w.lower())

for lid, line in lines.items():
    t = timings.get(lid)
    if not t or (only and lid not in only) or (not only and t.get("aligned") == t["hash"]):
        continue
    wav = subprocess.run(["ffmpeg", "-loglevel", "error", "-i", str(ROOT / f"public/audio/{lid}.wav"),
                          "-ar", "24000", "-ac", "1", "-f", "wav", "-"], capture_output=True, check=True).stdout
    lang = "fr" if lid == "d_french" else "en"
    q = urllib.parse.urlencode({"json_config": json.dumps({"language": lang})})
    req = urllib.request.Request(f"https://api.gradium.ai/api/post/speech/asr?{q}", data=wav,
                                 headers={"x-api-key": KEY, "Content-Type": "audio/wav"})
    heard = []
    with urllib.request.urlopen(req) as resp:
        for raw in resp:
            if raw.strip():
                m = json.loads(raw)
                if m.get("type") == "text":
                    heard.append({"text": m["text"], "start": m["start_s"]})
                elif m.get("type") == "end_text" and heard:
                    heard[-1]["end"] = m.get("stop_s")
    for i, w in enumerate(heard):
        w.setdefault("end", heard[i + 1]["start"] if i + 1 < len(heard) else t["duration"])
    script = re.sub(r"<break[^>]*/>", " ", line["text"]).split()
    a, b = [norm(w) for w in script], [norm(w["text"]) for w in heard]
    times = [None] * len(script)
    for blk in difflib.SequenceMatcher(None, a, b, autojunk=False).get_matching_blocks():
        for k in range(blk.size):
            times[blk.a + k] = heard[blk.b + k]["start"]
    # Interpolate the rest (fillers STT drops) between their matched neighbours.
    known = [(i, x) for i, x in enumerate(times) if x is not None] or [(0, 0.0)]
    for i in range(len(times)):
        if times[i] is None:
            prev = max((k for k in known if k[0] < i), default=(-1, 0.0))
            nxt = min((k for k in known if k[0] > i), default=(len(times), t["duration"] - 0.3))
            times[i] = prev[1] + (nxt[1] - prev[1]) * (i - prev[0]) / (nxt[0] - prev[0])
    t["heard"] = heard
    t["script"] = [{"text": w, "start": round(x, 3)} for w, x in zip(script, times)]
    t["aligned"] = t["hash"]
    print(lid, " ".join(f"{w['text']}@{w['start']:.2f}" for w in t["script"]))
path.write_text(json.dumps(timings, indent=1, ensure_ascii=False))

# TTS timestamps are exact where a segment is one word; STT's fill in inside the
# multi-word segments TTS emits first.
for lid, t in timings.items():
    if "script" not in t:
        continue
    flat = []  # (word, segment start, next segment start, single?)
    for i, seg in enumerate(t["words"]):
        ws = seg["text"].split()
        nxt = t["words"][i + 1]["start"] if i + 1 < len(t["words"]) else t["duration"]
        flat += [(w, seg["start"], nxt, len(ws) == 1) for w in ws]
    if len(flat) != len(t["script"]):
        print("skip merge", lid, len(flat), len(t["script"]))
        continue
    words, last = [], 0.0
    for (w, s, nxt, single), sw in zip(flat, t["script"]):
        x = s if single else min(max(sw["start"], s), nxt - 0.05)
        x = max(x, last)
        words.append({"text": sw["text"], "start": round(x, 3)})
        last = x
    t["timed"] = words
path.write_text(json.dumps(timings, indent=1, ensure_ascii=False))
print("merged")
