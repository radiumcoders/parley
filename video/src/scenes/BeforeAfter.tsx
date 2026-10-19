import React from "react";
import { AbsoluteFill, Easing, interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import pipeline from "../data/pipeline.json";
import { S } from "../lib/cues";
import { sec, words } from "../lib/timing";
import { C, MONO, SANS } from "../theme";

const B = S.beforeAfter;
const SAID = words("d_before").map((w) => w.text);
const READ = words("d_after");
const CLEANED = pipeline.before.cleaned.split(" ");

// Which spoken words the cleanup took out, and why.
type Kind = "filler" | "retract" | "file" | undefined;
// Um, so, can you, uh add rate limiting to the, the login endpoint? Like, five requests a
// minute. Wait, no, actually, make that ten. And, uh, put it in the auth file.
const FILLER = new Set([0, 1, 4, 9, 25]); // um, so, uh, the repeated "the", uh
const RETRACT = new Set([14, 18, 19, 20, 21, 22]); // five … wait, no, actually, make that
const kindOf = (i: number): Kind =>
  FILLER.has(i) ? "filler" : RETRACT.has(i) ? "retract" : i >= SAID.length - 3 ? "file" : undefined;

const MARKS = { filler: B.compare + sec(0.3), retract: B.compare + sec(1.3), file: B.compare + sec(3.4) };
const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
const ease = Easing.bezier(0.25, 0.1, 0.25, 1);

const Card: React.FC<{ label: string; at: number; children: React.ReactNode; tone?: "said" | "sent" }> = ({ label, at, children, tone }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({ fps, frame: frame - at, config: { damping: 200 } });
  return (
    <div
      style={{
        width: 1060,
        background: C.card,
        border: `1px solid ${tone === "sent" ? C.accent : C.line}`,
        borderRadius: 18,
        padding: "20px 28px 24px",
        boxShadow: "0 1px 2px rgba(0,0,0,0.06)",
        opacity: s,
        transform: `translateY(${(1 - s) * 24}px)`,
      }}
    >
      <div style={{ fontFamily: SANS, fontSize: 16, color: tone === "sent" ? C.accent : C.muted, marginBottom: 10, fontWeight: 500 }}>{label}</div>
      {children}
    </div>
  );
};

export const BeforeAfter: React.FC = () => {
  const frame = useCurrentFrame();
  const getsAt = B.gets + sec(0.69);
  return (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", gap: 22, paddingBottom: 60 }}>
      <Card label="What you said" at={2}>
        <div style={{ fontFamily: SANS, fontSize: 27, lineHeight: 1.5, color: C.ink, letterSpacing: "-0.01em" }}>
          {SAID.map((w, i) => {
            const k = kindOf(i);
            const m = k ? interpolate(frame, [MARKS[k], MARKS[k] + 10], [0, 1], { ...clamp, easing: ease }) : 0;
            const struck = k === "filler" || k === "retract";
            return (
              <span key={i} style={{ position: "relative", display: "inline-block", marginRight: "0.26em", color: struck ? (m > 0.5 ? C.faint : C.ink) : C.ink }}>
                {w}
                {struck && (
                  <span style={{ position: "absolute", left: -1, right: -1, top: "54%", height: 2.5, background: C.accent, transformOrigin: "left", transform: `scaleX(${m})`, borderRadius: 2 }} />
                )}
                {k === "file" && (
                  <span style={{ position: "absolute", left: 0, right: 0, bottom: 2, height: 3, background: C.accent, transformOrigin: "left", transform: `scaleX(${m})`, borderRadius: 2 }} />
                )}
              </span>
            );
          })}
        </div>
      </Card>
      {frame >= getsAt && (
        <Card label="What Claude gets" at={getsAt} tone="sent">
          <div style={{ fontFamily: SANS, fontSize: 30, lineHeight: 1.45, letterSpacing: "-0.015em", fontWeight: 500 }}>
            {CLEANED.map((w, i) => {
              // Karaoke over the clean read: the @path is read as "auth dot ts".
              const spoken = READ[Math.min(i, READ.length - 1)];
              const t = frame - B.after - spoken.at;
              const lit = interpolate(t, [0, 4], [0, 1], clamp);
              const isPath = w.startsWith("@");
              const fileMark = interpolate(frame, [MARKS.file, MARKS.file + 10], [0, 1], clamp);
              return (
                <span
                  key={i}
                  style={{
                    display: "inline-block",
                    marginRight: "0.26em",
                    color: isPath ? C.accent : C.ink,
                    opacity: 0.32 + 0.68 * lit,
                    fontFamily: isPath ? MONO : SANS,
                    fontSize: isPath ? 26 : undefined,
                    background: isPath ? `rgba(109,91,240,${0.06 + 0.1 * fileMark})` : undefined,
                    borderRadius: 6,
                    padding: isPath ? "0 6px" : undefined,
                  }}
                >
                  {w}
                </span>
              );
            })}
          </div>
        </Card>
      )}
    </AbsoluteFill>
  );
};
