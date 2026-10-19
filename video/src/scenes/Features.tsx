import React from "react";
import { AbsoluteFill, Easing, interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { Keystroke } from "@/components/remocn/keystroke";
import { TextReveal } from "@/components/snap-cn/text-reveal";
import pipeline from "../data/pipeline.json";
import { S } from "../lib/cues";
import { sec, words } from "../lib/timing";
import { C, MONO, SANS } from "../theme";
import { CLAUDE } from "../ui/ClaudeWindow";
import { Pill } from "../ui/Pill";
import { levelsAt } from "../ui/usePill";

const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
const ease = Easing.bezier(0.25, 0.1, 0.25, 1);

const useIn = (at: number) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return spring({ fps, frame: frame - at, config: { damping: 200 }, durationInFrames: 16 });
};

const card: React.CSSProperties = {
  background: C.card,
  border: `1px solid ${C.line}`,
  borderRadius: 16,
  boxShadow: "0 1px 2px rgba(0,0,0,0.06)",
};

// --- Spoken commands --------------------------------------------------------------

const CM = S.commands;
// Onsets in f_commands, measured from the audio.
const ROWS: { said: string; at: number; keys?: string; slash?: string }[] = [
  { said: "stop", at: CM.line + sec(0.16), keys: "esc" },
  { said: "undo that", at: CM.line + sec(0.95), keys: "esc esc" },
  { said: "clear the chat", at: CM.line + sec(1.72), slash: "/clear" },
  { said: "compact", at: CM.line + sec(2.78), slash: "/compact" },
];
const ROW_Y = (i: number) => 196 + i * 92;

const Row: React.FC<{ row: (typeof ROWS)[number]; i: number }> = ({ row, i }) => {
  const frame = useCurrentFrame();
  const s = useIn(row.at);
  const typed = row.slash ? row.slash.slice(0, Math.max(0, Math.floor((frame - row.at - 8) / 2))) : "";
  return (
    <div style={{ position: "absolute", top: ROW_Y(i) - 30, left: 250, width: 780, height: 60, display: "flex", alignItems: "center", opacity: s, transform: `translateX(${(1 - s) * -20}px)` }}>
      <div style={{ width: 330, fontFamily: SANS, fontSize: 34, fontWeight: 600, letterSpacing: "-0.02em", color: C.ink }}>“{row.said}”</div>
      <div style={{ width: 70, fontFamily: SANS, fontSize: 28, color: C.faint }}>→</div>
      {row.slash && (
        <div style={{ fontFamily: MONO, fontSize: 28, color: CLAUDE, background: "#FBF3EF", border: "1px solid #F1DDD3", borderRadius: 8, padding: "6px 14px", minWidth: 40 }}>
          {typed || " "}
        </div>
      )}
    </div>
  );
};

export const Commands: React.FC = () => (
  <AbsoluteFill>
    <AbsoluteFill style={{ alignItems: "center", paddingTop: 64 }}>
      <div style={{ fontFamily: SANS, fontSize: 18, color: C.muted }}>Say just one of these, and parley presses the keys</div>
    </AbsoluteFill>
    {ROWS.map((r, i) => (
      <Row key={r.said} row={r} i={i} />
    ))}
    <Keystroke steps={[{ at: ROWS[0].at + 8, keys: "esc" }]} theme="light" accent={C.accent} x={650 / 1280} y={ROW_Y(0) / 720} size={50} linger={400} />
    <Keystroke steps={[{ at: ROWS[1].at + 10, keys: "esc esc" }]} theme="light" accent={C.accent} x={650 / 1280} y={ROW_Y(1) / 720} size={50} linger={400} />
  </AbsoluteFill>
);

// --- Languages ----------------------------------------------------------------------

const LG = S.languages;
const LANGS: [string, number][] = [
  ["English", -1],
  ["Français", LG.line + sec(0.32)],
  ["Deutsch", LG.line + sec(0.8)],
  ["Español", LG.line + sec(1.25)],
  ["Português", LG.line + sec(1.9)],
];
const FR = words("d_french");
const EN = pipeline.french.heard;

export const Languages: React.FC = () => {
  const frame = useCurrentFrame();
  const englishAt = LG.line + sec(4.6);
  const englishOn = interpolate(frame, [englishAt, englishAt + 10], [0, 1], { ...clamp, easing: ease });
  const f = frame - LG.speak;
  // stt-translate writes English as you speak French: stream it over the clip.
  const enWords = EN.split(" ");
  const enShown = enWords.slice(0, Math.round(interpolate(f, [12, 150], [0, enWords.length], clamp))).join(" ");
  let pill: React.ReactNode = null;
  const s = { tick: frame, scale: 1.35 };
  if (frame >= LG.speak - 8 && frame < LG.release)
    pill = <Pill state="rec" text={enShown || "Listening…"} dim={!enShown} levels={levelsAt("d_french", f)} seconds={(frame - LG.speak + 8) / 30} {...s} />;
  else if (frame >= LG.cleaning && frame < LG.done) pill = <Pill state="work" text="Cleaning up…" detail={EN} {...s} />;
  else if (frame >= LG.done && frame < LG.done + 56) pill = <Pill state="done" text={pipeline.french.cleaned} {...s} />;
  const frenchOn = interpolate(frame, [LG.speak - 10, LG.speak + 4], [0, 1], clamp);
  const sentOn = interpolate(frame, [LG.done, LG.done + 12], [0, 1], { ...clamp, easing: ease });
  return (
    <AbsoluteFill>
      <AbsoluteFill style={{ alignItems: "center", paddingTop: 120, transform: `translateY(${-frenchOn * 60}px)` }}>
        <div style={{ display: "flex", gap: 14, alignItems: "center" }}>
          {LANGS.map(([name, at], i) => {
            const on = at < 0 ? 1 : interpolate(frame, [at, at + 8], [0, 1], clamp);
            const sIn = interpolate(frame, [4 + i * 4, 18 + i * 4], [0, 1], clamp);
            return (
              <div
                key={name}
                style={{
                  ...card,
                  borderRadius: 999,
                  padding: "10px 22px",
                  fontFamily: SANS,
                  fontSize: 24,
                  fontWeight: 500,
                  color: on > 0.5 ? C.ink : C.faint,
                  borderColor: on > 0.5 && at >= 0 ? C.ink : C.line,
                  opacity: sIn,
                  transform: `translateY(${(1 - sIn) * 10}px)`,
                }}
              >
                {name}
              </div>
            );
          })}
        </div>
        <div style={{ marginTop: 34, fontFamily: SANS, fontSize: 30, color: C.muted, opacity: englishOn, transform: `translateY(${(1 - englishOn) * 10}px)` }}>
          Claude gets <span style={{ color: C.accent, fontWeight: 600 }}>English</span>
        </div>
      </AbsoluteFill>
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", paddingTop: 70, opacity: frenchOn }}>
        <div style={{ width: 980, textAlign: "center", fontFamily: SANS, fontSize: 36, fontWeight: 500, letterSpacing: "-0.02em", lineHeight: 1.4, color: C.ink }}>
          {FR.map((w, i) => (
            <span key={i} style={{ opacity: interpolate(f - w.at, [0, 5], [0.2, 1], clamp) }}>
              {w.text}{" "}
            </span>
          ))}
        </div>
        <div style={{ marginTop: 22, fontFamily: SANS, fontSize: 26, color: C.accent, fontWeight: 500, opacity: sentOn, transform: `translateY(${(1 - sentOn) * 10}px)` }}>
          → {pipeline.french.cleaned}
        </div>
      </AbsoluteFill>
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "flex-end", paddingBottom: 30 }}>{pill}</AbsoluteFill>
    </AbsoluteFill>
  );
};

// --- Learns your words, keeps every turn --------------------------------------------

const LE = S.learn;

export const Learn: React.FC = () => {
  const frame = useCurrentFrame();
  const sel = LE.line + sec(1.1);
  const fixed = LE.line + sec(1.8);
  const saved = LE.line + sec(2.5);
  const selOn = interpolate(frame, [sel, sel + 8], [0, 1], clamp);
  const word = frame < fixed ? "Gradient" : "Gradium".slice(0, Math.max(1, Math.floor((frame - fixed) / 2) + 1));
  const fileIn = useIn(saved);
  const lineIn = interpolate(frame, [saved + 10, saved + 20], [0, 1], clamp);
  const toHistory = interpolate(frame, [LE.history - 12, LE.history + 6], [0, 1], { ...clamp, easing: ease });
  const term = useIn(LE.history);
  const lines = [
    { at: 8, text: "$ parley history 1", tone: C.ink },
    { at: 30, text: "  4m ago in com.mitchellh.ghostty", tone: C.muted },
    { at: 42, text: `  heard: ${pipeline.before.heard}`, tone: C.muted },
    { at: 54, text: `  ${pipeline.before.cleaned}`, tone: C.ink },
  ];
  return (
    <AbsoluteFill>
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", gap: 24, opacity: 1 - toHistory, transform: `translateY(${-toHistory * 60}px)` }}>
        <div style={{ ...card, width: 820, padding: "16px 22px", fontFamily: MONO, fontSize: 24, display: "flex", gap: 14 }}>
          <span style={{ color: C.muted }}>&gt;</span>
          <span>
            Make parley speak with{" "}
            <span style={{ background: frame < fixed ? `rgba(77,163,255,${0.28 * selOn})` : undefined, borderRadius: 4, color: frame >= fixed ? C.accent : C.ink }}>{word}</span>
            {frame >= fixed && frame < fixed + 16 && <span style={{ borderLeft: `2px solid ${C.ink}`, marginLeft: 1 }} />} voices
          </span>
        </div>
        <div style={{ ...card, width: 820, padding: "14px 22px", fontFamily: MONO, fontSize: 17, lineHeight: 1.7, opacity: fileIn, transform: `translateY(${(1 - fileIn) * 16}px)` }}>
          <div style={{ fontFamily: SANS, fontSize: 15, color: C.muted, marginBottom: 6 }}>~/.config/parley/words.txt</div>
          <div style={{ color: C.faint }}># Names and terms parley should get right, one per line</div>
          <div style={{ color: C.muted }}>Svelte: often heard as "Swell"</div>
          <div style={{ color: C.accent, opacity: lineIn, transform: `translateX(${(1 - lineIn) * -12}px)` }}>Gradium: often heard as "Gradient"</div>
        </div>
      </AbsoluteFill>
      {frame >= LE.history - 12 && (
        <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", opacity: term, transform: `translateY(${(1 - term) * 30}px)` }}>
          <div style={{ width: 1000, background: "#1B1A18", borderRadius: 14, padding: "22px 26px", fontFamily: MONO, fontSize: 18, lineHeight: 1.6, color: "#E8E5DD", boxShadow: "0 1px 2px rgba(0,0,0,0.1)" }}>
            {lines.map((l) =>
              frame >= LE.history + l.at ? (
                <div key={l.text} style={{ color: l.tone === C.ink ? "#E8E5DD" : "#8A857C", whiteSpace: "pre-wrap", paddingLeft: l.text.startsWith("  ") ? 18 : 0, textIndent: l.text.startsWith("  ") ? -18 : 0 }}>
                  {l.text}
                </div>
              ) : null,
            )}
          </div>
        </AbsoluteFill>
      )}
    </AbsoluteFill>
  );
};

// --- Outro --------------------------------------------------------------------------

const O = S.outro;

export const Outro: React.FC = () => {
  const frame = useCurrentFrame();
  const pillIn = useIn(O.line + sec(0.6));
  const foot = interpolate(frame, [O.line + sec(1.6), O.line + sec(2.2)], [0, 1], clamp);
  return (
    <AbsoluteFill>
      <AbsoluteFill style={{ transform: "translateY(-50px)" }}>
        <TextReveal text="parley" fontSize={150} fontFamily={SANS} color={C.ink} fontWeight={600} />
      </AbsoluteFill>
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", paddingTop: 300 }}>
        <div style={{ opacity: pillIn, transform: `translateY(${(1 - pillIn) * 16}px)` }}>
          <Pill state="done" text="Just talk to Claude Code." tick={frame} scale={1.5} />
        </div>
      </AbsoluteFill>
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "flex-end", paddingBottom: 56, opacity: foot }}>
        <div style={{ fontFamily: SANS, fontSize: 18, color: C.muted }}>
          Speech by <span style={{ color: C.ink, fontWeight: 600 }}>Gradium</span> · built in Rust, for Claude Code on Linux
        </div>
      </AbsoluteFill>
    </AbsoluteFill>
  );
};
