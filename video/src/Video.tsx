import React from "react";
import { AbsoluteFill, Sequence, staticFile } from "remotion";
import { Audio } from "@remotion/media";
import { springTiming, TransitionSeries } from "@remotion/transitions";
import { slide } from "@remotion/transitions/slide";
import { Backdrop } from "@/components/remocn/backdrop";
import { WordCaptions } from "@/components/snap-cn/word-captions";
import { AUDIO, CAPTIONS, ORDER, S, START, T, type SceneKey } from "./lib/cues";
import { AskBack } from "./scenes/AskBack";
import { BeforeAfter } from "./scenes/BeforeAfter";
import { Commands, Languages, Learn, Outro } from "./scenes/Features";
import { Hook } from "./scenes/Hook";
import { Positioning } from "./scenes/Positioning";
import { Reveal } from "./scenes/Reveal";
import { C, SANS } from "./theme";

const SCENES: Record<SceneKey, React.FC> = {
  hook: Hook,
  positioning: Positioning,
  reveal: Reveal,
  beforeAfter: BeforeAfter,
  askBack: AskBack,
  commands: Commands,
  languages: Languages,
  learn: Learn,
  outro: Outro,
};

// Slides, never fades: neighbouring beats overlap for T frames (snapcn anti-pattern 12).
const beat = {
  presentation: slide({ direction: "from-right" }),
  timing: springTiming({ durationInFrames: T, config: { damping: 200 } }),
};

// Lines the screen already spells out word for word get no caption.
const ON_SCREEN = new Set(["n_problem1", "n_intro", "n_how", "n_outro"]);
const inScene = (k: SceneKey, f: number) => f >= START[k] && f < START[k] + S[k].dur;
const captions = CAPTIONS.filter((c) => !ON_SCREEN.has(c.clip));
// Where the pill sits at the bottom, the captions go to the top.
const top = captions.filter((c) => inScene("askBack", c.startFrame));
const bottom = captions.filter((c) => !inScene("askBack", c.startFrame));

const Captions: React.FC<{ words: typeof captions; atTop?: boolean }> = ({ words, atTop }) => (
  <AbsoluteFill style={atTop ? { transform: "translateY(-470px)" } : undefined}>
    <WordCaptions
      words={words.map(({ text, startFrame, endFrame }) => ({ text, startFrame, endFrame }))}
      preset="clean"
      aspect="16:9"
      fontSize={26}
      fontFamily={SANS}
      maxWords={7}
      mode="light"
      accentColor={C.accent}
    />
  </AbsoluteFill>
);

export const Video: React.FC = () => (
  <Backdrop fill={{ type: "color", value: C.paper }} padding={0} radius={0} shadow="">
    <TransitionSeries>
      {ORDER.flatMap((k, i) => {
        const Scene = SCENES[k];
        const seq = (
          <TransitionSeries.Sequence key={k} durationInFrames={S[k].dur}>
            <Scene />
          </TransitionSeries.Sequence>
        );
        return i === 0 ? [seq] : [<TransitionSeries.Transition key={`${k}-t`} {...beat} />, seq];
      })}
    </TransitionSeries>
    <Captions words={bottom} />
    <Captions words={top} atTop />
    {AUDIO.map((a, i) => (
      <Sequence key={i} from={a.at} layout="none">
        <Audio src={staticFile(`audio/${a.src}.wav`)} volume={a.volume ?? 1} />
      </Sequence>
    ))}
  </Backdrop>
);
