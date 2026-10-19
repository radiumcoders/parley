"""parley's chimes (the notes and fades of src/audio.rs, louder for video) and the
microphone level per video frame for the spoken clips, which drive the pill's bars."""
import json, math, pathlib, struct, subprocess, wave

ROOT = pathlib.Path(__file__).resolve().parent.parent
RATE, FADE, VOLUME = 48000, 0.01, 0.3
CHIMES = {"listen": [(880, 0.07)], "done": [(660, 0.07)], "ask": [(660, 0.09), (990, 0.12)], "finished": [(523, 0.14)]}
for name, notes in CHIMES.items():
    frames = bytearray()
    for freq, secs in notes:
        n = int(secs * RATE)
        for i in range(n):
            t = i / RATE
            env = max(0.0, min(1.0, t / FADE, (secs - t) / FADE))
            frames += struct.pack("<h", int(math.sin(2 * math.pi * freq * t) * env * VOLUME * 32767))
    with wave.open(str(ROOT / f"public/audio/chime_{name}.wav"), "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(RATE); w.writeframes(bytes(frames))

FPS = 30
levels = {}
for clip in ["d_before", "d_french", "d_reply", "n_how", "n_intro", "n_outro"]:
    pcm = subprocess.run(["ffmpeg", "-loglevel", "error", "-i", str(ROOT / f"public/audio/{clip}.wav"),
                          "-f", "s16le", "-ac", "1", "-ar", "48000", "-"], capture_output=True, check=True).stdout
    samples = struct.unpack(f"<{len(pcm)//2}h", pcm)
    step = RATE // FPS
    out = []
    for i in range(0, len(samples), step):
        chunk = samples[i:i + step]
        rms = math.sqrt(sum(s * s for s in chunk) / max(1, len(chunk))) / 32768
        db = 20 * math.log10(max(rms, 1e-6))
        out.append(round(max(0.0, min(1.0, (db + 50) / 38)), 3))  # -50 dBFS..-12 dBFS -> 0..1
    levels[clip] = out
(ROOT / "src/data/levels.json").write_text(json.dumps(levels))
print({k: len(v) for k, v in levels.items()})
