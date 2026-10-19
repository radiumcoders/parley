import React from "react";
import { AbsoluteFill, Easing, interpolate, useCurrentFrame } from "remotion";
import { Keystroke } from "@/components/remocn/keystroke";
import pipeline from "../data/pipeline.json";
import { S } from "../lib/cues";
import { words } from "../lib/timing";
import { C, SANS } from "../theme";
import { Appear, CLAUDE, ClaudeWindow, UserTurn, Working } from "../ui/ClaudeWindow";
import { Pill } from "../ui/Pill";
import { heardAt, levelsAt } from "../ui/usePill";

const R = S.reveal;
const SAID = words("d_before");
const HEARD_ALL = pipeline.before.heard;
const CLEANED = pipeline.before.cleaned;

/** The cleaned prompt with its @path the way Claude Code colours a file mention. */
export const withPaths = (text: string) =>
  text.split(/(@[\w./-]+\w)/).map((part, i) =>
    part.startsWith("@") ? (
      <span key={i} style={{ color: CLAUDE }}>{part}</span>
    ) : (
      <React.Fragment key={i}>{part}</React.Fragment>
    ),
  );

const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
const ease = Easing.bezier(0.45, 0, 0.2, 1);

/** Keyframed camera: [frame, zoom, focus x, focus y] (frame coords), eased between. */
const camera = (frame: number, keys: [number, number, number, number][]) => {
  const at = keys.map((k) => k[0]);
  const v = (i: number) => interpolate(frame, at, keys.map((k) => k[i]), { ...clamp, easing: ease });
  return { zoom: v(1), x: v(2), y: v(3) };
};

export const PillFor: React.FC<{ frame: number }> = ({ frame }) => {
  const f = frame - R.speak;
  if (frame < R.press || frame >= R.done + 56) return null;
  const s = { tick: frame, scale: 1.35 };
  if (frame < R.release) {
    const heard = heardAt("d_before", f);
    return <Pill state="rec" text={heard || "Listening…"} dim={!heard} levels={levelsAt("d_before", f)} seconds={(frame - R.press) / 30} {...s} />;
  }
  if (frame < R.cleaning) return <Pill state="work" text="Transcribing…" detail={HEARD_ALL} {...s} />;
  if (frame < R.done) return <Pill state="work" text="Cleaning up…" detail={HEARD_ALL} {...s} />;
  return <Pill state="done" text={CLEANED} {...s} />;
};

export const Reveal: React.FC = () => {
  const frame = useCurrentFrame();
  const cam = camera(frame, [
    [0, 1, 640, 360],
    [R.speak + 6, 1, 640, 360],
    [R.speak + 30, 1.3, 640, 640],
    [R.release - 4, 1.3, 640, 640],
    [R.release + 22, 1, 640, 360],
    [R.done - 6, 1, 640, 360],
    [R.done + 14, 1.3, 640, 470],
    [R.enter + 30, 1.3, 640, 470],
    [R.enter + 56, 1, 640, 360],
  ]);
  const pushed = interpolate(frame, [R.speak + 6, R.speak + 30, R.release - 4, R.release + 22], [0, 1, 1, 0], { ...clamp, easing: ease });
  const sending = frame >= R.done && frame < R.enter;
  const sent = frame >= R.enter;

  // What was said, word by word: the before.
  const said = SAID.filter((w) => w.at <= frame - R.speak);
  const saidOpacity = interpolate(frame, [R.speak + 14, R.speak + 30, R.release, R.release + 14], [0, 1, 1, 0], clamp);

  return (
    <AbsoluteFill>
      <AbsoluteFill
        style={{
          transform: `translate(${640 - cam.x}px, ${360 - cam.y}px) scale(${cam.zoom})`,
          transformOrigin: `${cam.x}px ${cam.y}px`,
        }}
      >
        <AbsoluteFill style={{ alignItems: "center", paddingTop: 34, opacity: 1 - pushed * 0.88 }}>
          <ClaudeWindow
            enterAt={0}
            width={1000}
            height={540}
            prompt={sending ? withPaths(CLEANED) : undefined}
          >
            {sent && (
              <>
                <Appear at={R.enter}>
                  <UserTurn>{withPaths(CLEANED)}</UserTurn>
                </Appear>
                <Appear at={R.enter + 8}>
                  <Working verb="Thinking" from={R.enter + 8} />
                </Appear>
              </>
            )}
          </ClaudeWindow>
        </AbsoluteFill>
        <AbsoluteFill style={{ alignItems: "center", justifyContent: "flex-end", paddingBottom: 30 }}>
          <PillFor frame={frame} />
        </AbsoluteFill>
      </AbsoluteFill>

      {/* The words as they were said, above the zoomed pill. */}
      <AbsoluteFill style={{ alignItems: "center", paddingTop: 70, opacity: saidOpacity }}>
        <div style={{ fontFamily: SANS, fontSize: 18, color: C.muted, marginBottom: 14 }}>What you said</div>
        <div style={{ width: 1020, textAlign: "center", fontFamily: SANS, fontSize: 36, lineHeight: 1.42, fontWeight: 500, letterSpacing: "-0.02em", color: C.ink }}>
          {said.map((w, i) => {
            const age = frame - R.speak - w.at;
            return (
              <span key={i} style={{ opacity: interpolate(age, [0, 5], [0.2, 1], clamp) }}>
                {w.text}{" "}
              </span>
            );
          })}
        </div>
      </AbsoluteFill>

      <Keystroke
        steps={[{ at: R.press, keys: "f7", hold: R.release - R.press }]}
        theme="light"
        accent={C.accent}
        x={0.075}
        y={0.88}
        size={54}
        linger={20}
      />
    </AbsoluteFill>
  );
};
