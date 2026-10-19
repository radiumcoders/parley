"""Synthesise every line of the video with Gradium TTS.

Reads scripts/lines.json ({id: {voice, text}}), writes public/audio/<id>.wav and
src/data/timings.json ({id: {duration, words: [{text, start, end}]}}). Skips lines whose
text hasn't changed since the last run. GRADIUM_API_KEY from ~/.config/parley/env.
"""
import base64, hashlib, json, os, pathlib, subprocess, sys, urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
ENV = pathlib.Path.home() / ".config/parley/env"
KEY = next(l.split("=", 1)[1].strip() for l in ENV.read_text().splitlines() if l.startswith("GRADIUM_API_KEY="))

lines = json.loads((ROOT / "scripts/lines.json").read_text())
out_dir = ROOT / "public/audio"
data_path = ROOT / "src/data/timings.json"
data_path.parent.mkdir(parents=True, exist_ok=True)
timings = json.loads(data_path.read_text()) if data_path.exists() else {}
only = set(sys.argv[1:])

for lid, line in lines.items():
    if only and lid not in only:
        continue
    digest = hashlib.sha1(json.dumps(line, sort_keys=True).encode()).hexdigest()
    wav = out_dir / f"{lid}.wav"
    if not only and timings.get(lid, {}).get("hash") == digest and wav.exists():
        continue
    body = {"text": line["text"], "voice_id": line["voice"], "output_format": "wav", "only_audio": False}
    if "config" in line:
        body["json_config"] = line["config"]
    req = urllib.request.Request(
        "https://api.gradium.ai/api/post/speech/tts",
        data=json.dumps(body).encode(),
        headers={"x-api-key": KEY, "Content-Type": "application/json"},
    )
    audio, words = b"", []
    with urllib.request.urlopen(req) as resp:
        for raw in resp:
            if not raw.strip():
                continue
            msg = json.loads(raw)
            if msg.get("type") == "audio":
                audio += base64.b64decode(msg["audio"])
            elif msg.get("type") == "text":
                words.append({"text": msg["text"], "start": msg["start_s"], "end": msg.get("stop_s", msg["start_s"])})
            elif msg.get("type") == "error":
                sys.exit(f"{lid}: {msg}")
    tmp = out_dir / f"{lid}.raw.wav"
    tmp.write_bytes(audio)
    # Normalise loudness so narrator and speaker clips sit at one level.
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", str(tmp), "-af", "loudnorm=I=-16:TP=-1.5",
                    "-ar", "48000", "-ac", "1", str(wav)], check=True)
    tmp.unlink()
    dur = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(wav)],
                               capture_output=True, text=True).stdout)
    timings[lid] = {"hash": digest, "duration": round(dur, 3), "words": words}
    print(f"{lid}: {dur:.2f}s, {len(words)} words")

data_path.write_text(json.dumps(timings, indent=1))
