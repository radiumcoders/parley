import React from "react";
import { AbsoluteFill, Easing, interpolate, Sequence, useCurrentFrame } from "remotion";
import { TextReveal } from "@/components/snap-cn/text-reveal";
import { S } from "../lib/cues";
import { sec } from "../lib/timing";
import { C, SANS } from "../theme";

const { p2, p3, dur } = S.hook;

// What plain dictation writes down: everything, fillers and all.
type Kind = "filler" | "repeat" | "retract" | undefined;
const RAW: [string, Kind][] = [
  ["um", "filler"], ["so", undefined], ["can", undefined], ["you", undefined], ["uh", "filler"],
  ["add", undefined], ["rate", undefined], ["limiting", undefined], ["to", undefined],
  ["the", "repeat"], ["the", undefined], ["login", undefined], ["endpoint", undefined],
  ["like", "filler"], ["five", "retract"], ["requests", "retract"], ["a", "retract"],
  ["minute", "retract"], ["wait", "retract"], ["no", "retract"], ["actually", "retract"],
  ["make", undefined], ["that", undefined], ["ten", undefined], ["and", undefined],
  ["uh", "filler"], ["put", undefined], ["it", undefined], ["in", undefined],
  ["the", undefined], ["auth", undefined], ["file", undefined],
];

// Narrator beats inside n_problem2 (onsets measured from the audio).
const WRITE_AT = p2 + sec(1.63);
const MARK: Record<Exclude<Kind, undefined>, number> = {
  filler: p2 + sec(4.3),
  repeat: p2 + sec(5.51),
  retract: p2 + sec(7.08),
};
const FIX_AT = p3 + 8;

const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
const ease = Easing.bezier(0.25, 0.1, 0.25, 1);

const DictationCard: React.FC = () => {
  const frame = useCurrentFrame();
  const enter = interpolate(frame, [p2 - 4, p2 + 14], [0, 1], { ...clamp, easing: ease });
  // Fixing by hand: the marked words go one at a time, slowly.
  const marked = RAW.map(([, k], i) => (k ? i : -1)).filter((i) => i >= 0);
  const said = Math.min(14, Math.max(0, Math.floor((frame - WRITE_AT) / 30) + 9));
  const fixing = Math.max(0, frame - FIX_AT);
  return (
    <div
      style={{
        width: 820,
        background: C.card,
        border: `1px solid ${C.line}`,
        borderRadius: 18,
        boxShadow: "0 1px 2px rgba(0,0,0,0.06)",
        padding: "22px 30px 28px",
        opacity: enter,
        transform: `translateY(${(1 - enter) * 30}px)`,
        fontFamily: SANS,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10, color: C.muted, fontSize: 16, marginBottom: 16 }}>
        <div style={{ width: 9, height: 9, borderRadius: 5, background: frame < FIX_AT ? C.rec : C.faint }} />
        <span>Dictation</span>
        <span style={{ marginLeft: "auto", fontFeatureSettings: "'tnum'" }}>
          {frame < FIX_AT ? `0:${String(said).padStart(2, "0")}` : `fixing 0:${String(14 + Math.floor(fixing / 3)).padStart(2, "0")}`}
        </span>
      </div>
      <div style={{ fontSize: 31, lineHeight: 1.5, color: C.ink, letterSpacing: "-0.01em" }}>
        {RAW.map(([w, kind], i) => {
          const shown = frame >= WRITE_AT + i * 1.7;
          if (!shown) return null;
          const m = kind ? interpolate(frame, [MARK[kind], MARK[kind] + 8], [0, 1], { ...clamp, easing: ease }) : 0;
          const order = marked.indexOf(i);
          const goneAt = FIX_AT + 10 + order * 6;
          const gone = kind ? interpolate(frame, [goneAt, goneAt + 6], [0, 1], clamp) : 0;
          return (
            <span
              key={i}
              style={{
                display: "inline-block",
                marginRight: gone ? `${0.27 * (1 - gone)}em` : "0.27em",
                maxWidth: kind ? `${(1 - gone) * 8}em` : undefined,
                opacity: 1 - gone,
                whiteSpace: "nowrap",
                overflow: kind ? "hidden" : undefined,
                verticalAlign: "bottom",
                position: "relative",
                color: m > 0.5 ? C.accent : C.ink,
              }}
            >
              <span
                style={{
                  position: "absolute",
                  left: -3,
                  right: -3,
                  top: "14%",
                  bottom: "10%",
                  background: C.accentSoft,
                  borderRadius: 6,
                  transformOrigin: "left",
                  transform: `scaleX(${m})`,
                }}
              />
              <span style={{ position: "relative", textDecoration: kind === "retract" && m > 0.5 ? "line-through" : undefined }}>{w}</span>
            </span>
          );
        })}
      </div>
    </div>
  );
};

export const Hook: React.FC = () => {
  const frame = useCurrentFrame();
  const out = interpolate(frame, [p2 - 10, p2 + 6], [0, 1], { ...clamp, easing: ease });
  return (
    <AbsoluteFill>
      <Sequence durationInFrames={p2 + 8}>
        <AbsoluteFill style={{ transform: `translateY(${-out * 80}px)`, opacity: 1 - out }}>
          <TextReveal text="You think faster than you type." fontSize={64} fontFamily={SANS} color={C.ink} />
        </AbsoluteFill>
      </Sequence>
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", paddingBottom: 40 }}>
        {frame >= p2 - 4 && <DictationCard />}
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

export const HOOK_DURATION = dur;
