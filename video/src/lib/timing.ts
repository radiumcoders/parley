import timings from "../data/timings.json";
import { FPS } from "../theme";

type Word = { text: string; start: number };
type Clip = { duration: number; timed?: Word[]; heard?: (Word & { end: number })[] };
const T = timings as unknown as Record<string, Clip>;

export const sec = (s: number) => Math.round(s * FPS);
/** A clip's length in frames. */
export const clipFrames = (id: string) => Math.ceil(T[id].duration * FPS);
/** The script's words with their start frame, fillers included. */
export const words = (id: string) =>
  (T[id].timed ?? []).map((w) => ({ text: w.text, at: w.start * FPS }));
/** Start frame of the nth word of a clip (from 0). */
export const wordAt = (id: string, n: number) => (T[id].timed ?? [])[n].start * FPS;
/** Start frame of the first word matching `text` (after `from`). */
export const findWord = (id: string, text: string, from = 0) => {
  const ws = T[id].timed ?? [];
  const i = ws.findIndex((w, k) => k >= from && w.text.toLowerCase().replace(/[^\w']/g, "") === text);
  return i < 0 ? 0 : ws[i].start * FPS;
};
