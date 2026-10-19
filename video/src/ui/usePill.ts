import levelsJson from "../data/levels.json";
import timings from "../data/timings.json";
import { FPS } from "../theme";

const LEVELS = levelsJson as Record<string, number[]>;
type Heard = { text: string; start: number }[];
const HEARD = Object.fromEntries(
  Object.entries(timings as unknown as Record<string, { heard?: Heard }>).map(([k, v]) => [k, v.heard ?? []]),
);

/** The last 24 microphone levels at `frame` frames into `clip` (0 before and after). */
export const levelsAt = (clip: string, frame: number) => {
  const l = LEVELS[clip] ?? [];
  return Array.from({ length: 24 }, (_, i) => l[Math.floor(frame) - 23 + i] ?? 0);
};

/** What Gradium STT had heard `frame` frames into `clip`, the way the pill streams it. */
export const heardAt = (clip: string, frame: number, lagFrames = 6) =>
  HEARD[clip]
    .filter((w) => w.start * FPS + lagFrames <= frame)
    .map((w) => w.text)
    .join(" ");
