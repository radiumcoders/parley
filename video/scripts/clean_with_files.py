"""The first cleanup checkpoint from src/cleanup.rs (same rules, same model), with a
<files> list, which `parley replay` doesn't pass. Usage: clean_with_files.py TEXT FILE..."""
import json, pathlib, re, sys, urllib.request

src = (pathlib.Path(__file__).resolve().parents[2] / "src/cleanup.rs").read_text()
system = re.search(r'const SYSTEM: &str = r###"(.*?)"###;', src, re.S).group(1)
model = re.search(r'const MODEL: &str = "(.*?)"', src).group(1)
env = (pathlib.Path.home() / ".config/parley/env").read_text().splitlines()
key = next(l.split("=", 1)[1].strip() for l in env if l.startswith("ANTHROPIC_API_KEY="))

def turn(text, files):
    t = f"<dictation>{text}</dictation>"
    return t + ("\n<files>\n" + "\n".join(files) + "\n</files>" if files else "")

shots = [
    ("can you refer to the settings file and check why it crashes on mac",
     "Can you refer to @src/settings.py and check why it crashes on Mac?",
     ["src/settings.py", "tests/test_settings.py", "docs/settings.md"]),
    ("Um, can you add the retry to the upload function? No, actually, the download function.",
     "Can you add the retry to the download function?", []),
]
msgs = []
for raw, out, files in shots:
    msgs += [{"role": "user", "content": turn(raw, files)}, {"role": "assistant", "content": out}]
msgs.append({"role": "user", "content": turn(sys.argv[1], sys.argv[2:])})
req = urllib.request.Request("https://api.anthropic.com/v1/messages", data=json.dumps(
    {"model": model, "max_tokens": 400, "system": system, "messages": msgs}).encode(),
    headers={"x-api-key": key, "anthropic-version": "2023-06-01", "content-type": "application/json"})
print(json.load(urllib.request.urlopen(req))["content"][0]["text"])
