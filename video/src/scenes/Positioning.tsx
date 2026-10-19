import React from "react";
import { AbsoluteFill, Easing, interpolate, Sequence, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { TextReveal } from "@/components/snap-cn/text-reveal";
import { Keystroke } from "@/components/remocn/keystroke";
import { S } from "../lib/cues";
import { sec } from "../lib/timing";
import { C, SANS } from "../theme";
import { Pill } from "../ui/Pill";
import { levelsAt } from "../ui/usePill";

const { intro, how, dur } = S.positioning;
// Onsets in n_how, measured from the audio.
const HOLD = how + sec(0.05);
const TALK = how + sec(1.45);
const LETGO = how + sec(2.47);
const RELEASE = LETGO + 14;

const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;

export const Positioning: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const swap = interpolate(frame, [how - 14, how + 4], [0, 1], { ...clamp, easing: Easing.bezier(0.4, 0, 0.2, 1) });

  // Part one: the wordmark, with the pill as its tagline.
  const pillIn = spring({ fps, frame: frame - (intro + sec(0.5)), config: { damping: 200 } });
  const gradium = interpolate(frame, [intro + sec(3.0), intro + sec(3.5)], [0, 1], clamp);

  // Part two: hold F7, talk, let go — the pill follows.
  let pill: React.ReactNode = null;
  if (frame >= HOLD) {
    const f = frame - how;
    if (frame < TALK) pill = <Pill state="rec" text="Listening…" dim tick={frame} seconds={(frame - HOLD) / 30} scale={1.6} />;
    else if (frame < RELEASE)
      pill = <Pill state="rec" text={frame < LETGO ? "Talk." : "Talk. Let go."} levels={levelsAt("n_how", f)} tick={frame} seconds={(frame - HOLD) / 30} scale={1.6} />;
    else if (frame < RELEASE + 26) pill = <Pill state="work" text="Cleaning up…" detail="Talk. Let go." tick={frame} scale={1.6} />;
    else pill = <Pill state="done" text="Talk. Let go." tick={frame} scale={1.6} />;
  }
  const phrase = (text: string, at: number) => {
    const on = interpolate(frame, [at, at + 6], [0, 1], clamp);
    return (
      <span style={{ color: on > 0.5 ? C.ink : C.faint, transform: `translateY(${(1 - on) * 6}px)`, display: "inline-block" }}>{text}</span>
    );
  };

  return (
    <AbsoluteFill>
      <Sequence durationInFrames={how + 6}>
        <AbsoluteFill style={{ opacity: 1 - swap, transform: `translateY(${-swap * 60}px)` }}>
          <AbsoluteFill style={{ transform: "translateY(-50px)" }}>
            <TextReveal text="parley" fontSize={150} fontFamily={SANS} color={C.ink} fontWeight={600} />
          </AbsoluteFill>
          <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", paddingTop: 300 }}>
            <div style={{ opacity: pillIn, transform: `translateY(${(1 - pillIn) * 16}px)` }}>
              <Pill
                state="rec"
                text="Push-to-talk for Claude Code"
                levels={levelsAt("n_intro", frame - intro)}
                tick={frame}
                seconds={Math.max(0, frame - intro) / 30}
                scale={1.5}
              />
            </div>
            <div style={{ marginTop: 26, fontFamily: SANS, fontSize: 20, color: C.muted, opacity: gradium }}>
              Speech by <span style={{ color: C.ink, fontWeight: 600 }}>Gradium</span>
            </div>
          </AbsoluteFill>
        </AbsoluteFill>
      </Sequence>
      <Sequence from={how - 4}>
        <AbsoluteFill style={{ opacity: swap }}>
          <Keystroke steps={[{ at: HOLD - how + 4, keys: "f7", hold: RELEASE - HOLD }]} theme="light" accent={C.accent} x={0.5} y={0.36} size={96} linger={40} />
          <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", paddingTop: 60, fontFamily: SANS, fontSize: 52, fontWeight: 600, letterSpacing: "-0.03em", gap: 0 }}>
            <div style={{ display: "flex", gap: 28 }}>
              {phrase("Hold a key.", HOLD)}
              {phrase("Talk.", TALK)}
              {phrase("Let go.", LETGO)}
            </div>
          </AbsoluteFill>
          <AbsoluteFill style={{ alignItems: "center", justifyContent: "flex-end", paddingBottom: 70 }}>{pill}</AbsoluteFill>
        </AbsoluteFill>
      </Sequence>
    </AbsoluteFill>
  );
};

export const POSITIONING_DURATION = dur;
