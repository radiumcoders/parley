import React from "react";
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion";
import pipeline from "../data/pipeline.json";
import { S } from "../lib/cues";
import { words } from "../lib/timing";
import { Appear, ClaudeTurn, ClaudeWindow, UserTurn, Working } from "../ui/ClaudeWindow";
import { Pill } from "../ui/Pill";
import { heardAt, levelsAt } from "../ui/usePill";
import { withPaths } from "./Reveal";

const A = S.askBack;
const REPLY = pipeline.reply.cleaned;
const NOTE = pipeline.reply.voiceNote.trim();
const ANSWER = words("c_answer");
const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;

const PillFor: React.FC<{ frame: number }> = ({ frame }) => {
  const s = { tick: frame, scale: 1.35 };
  if (frame >= A.chime && frame < A.replyEnd) {
    const f = frame - A.reply;
    const heard = f > 0 ? heardAt("d_reply", f) : "";
    return <Pill state="answer" text={heard || "Your answer…"} dim={!heard} levels={levelsAt("d_reply", f)} seconds={(frame - A.chime) / 30} {...s} />;
  }
  if (frame >= A.cleaning && frame < A.done) return <Pill state="work" text="Cleaning up…" detail={pipeline.reply.heard} {...s} />;
  if (frame >= A.done && frame < A.done + 50) return <Pill state="done" text={REPLY} {...s} />;
  if (frame >= A.speak && frame < A.speak + 170) return <Pill state="speak" text="Reading the answer" detail="press the key to stop" {...s} />;
  return null;
};

export const AskBack: React.FC = () => {
  const frame = useCurrentFrame();
  const sending = frame >= A.done && frame < A.enter;
  return (
    <AbsoluteFill>
      <AbsoluteFill style={{ alignItems: "center", paddingTop: 34 }}>
        <ClaudeWindow
          welcome={false}
          width={1000}
          height={540}
          prompt={
            sending ? (
              <>
                {REPLY}
                <span style={{ opacity: 0.45 }}> {NOTE}</span>
              </>
            ) : undefined
          }
        >
          <UserTurn>{withPaths(pipeline.before.cleaned)}</UserTurn>
          <ClaudeTurn tool detail="Added a limiter: 10 requests a minute per client on POST /login">
            <b>Update</b>(src/auth.ts)
          </ClaudeTurn>
          <Appear at={A.question}>
            <ClaudeTurn>Rate limiting is in. Should I add a test for it too?</ClaudeTurn>
          </Appear>
          {frame >= A.enter && (
            <Appear at={A.enter}>
              <UserTurn>
                {REPLY}
                <span style={{ opacity: 0.45 }}> {NOTE}</span>
              </UserTurn>
            </Appear>
          )}
          {frame >= A.enter + 6 && frame < A.speak - 4 && (
            <Appear at={A.enter + 6}>
              <Working verb="Testing" from={A.enter + 6} />
            </Appear>
          )}
          {frame >= A.enter + 50 && (
            <Appear at={A.enter + 50}>
              <ClaudeTurn tool detail="Added 1 test · 1 passed">
                <b>Bash</b>(npm test -- auth)
              </ClaudeTurn>
            </Appear>
          )}
          {frame >= A.speak - 4 && (
            <Appear at={A.speak - 4}>
              <ClaudeTurn>
                {ANSWER.map((w, i) => (
                  <span key={i} style={{ opacity: interpolate(frame - A.speak - w.at, [0, 4], [0.3, 1], clamp) }}>
                    {w.text}{" "}
                  </span>
                ))}
              </ClaudeTurn>
            </Appear>
          )}
        </ClaudeWindow>
      </AbsoluteFill>
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "flex-end", paddingBottom: 30 }}>
        <PillFor frame={frame} />
      </AbsoluteFill>
    </AbsoluteFill>
  );
};
