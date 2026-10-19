"use client";

import { loadFont as loadSans } from "@remotion/google-fonts/Figtree";
import { useEffect, useMemo, useState } from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import {
  resolveFont,
  type SnapCnTheme,
  useSnapCnTheme,
} from "@/lib/snap-cn-ui";

/**
 * A sentence that arrives one word at a time, out of order, and gathers.
 *
 * The first word holds the middle of the frame on its own. Then the rest of the
 * sentence drops in around it — not in reading order, and not where it belongs
 * — each word landing somewhere near its slot in the accent colour and sliding
 * the last of the way home, turning to ink as it settles. The line makes room
 * for them as they come, and when the last one is down the whole block eases
 * back to its real size.
 *
 * ## Two layouts, and one word that moves between them
 *
 * The scene is only ever in one of two arrangements: the lead word alone,
 * centred on the card, or the whole sentence wrapped and centred. Every other
 * word appears in the second one and never sees the first. The lead word is the
 * only thing that has to travel between them — and it does so on two separate
 * clocks, which is measured, not invented: it drops onto its line in seven
 * frames and slides left across the line over twenty-one, because one of those
 * is a line changing and the other is a line reflowing.
 *
 * ## The layout is arithmetic, not the browser's
 *
 * Word advances come from `measureText` and the wrap, the centring and every
 * slot are computed here. That is not a preference either: a word has to be
 * transformed from somewhere else to its slot, and to know where its slot *is*
 * you have to have laid the line out yourself. It also means the line break
 * cannot move on you frame to frame — the layout is solved once, at scale 1,
 * and the block's own scale is a transform over the top of it.
 */

const { fontFamily: SANS } = loadSans("normal", {
  weights: ["400", "500", "600"],
  subsets: ["latin"],
});

/* ─────────────────────────────────────────────────────────────────────────
   The card, in the reference's own 710 × 388 pixels
   ───────────────────────────────────────────────────────────────────────── */

export const REF_W = 710;
export const REF_H = 388;
export const CENTRE_X = REF_W / 2;
/**
 * Where the lines are centred, which is not quite the middle of the card.
 *
 * A line is centred on its advance box and read back as its ink, and the two
 * differ by half the difference of the first character's left bearing and the
 * last one's right — a face-dependent three and a half pixels here.
 */
export const LINE_CX = REF_W / 2 - 3.53;
export const CENTRE_Y = REF_H / 2;
/** The card's corner, measured off the coverage of its own antialiasing. */
export const RADIUS = 5.5;
/** The recording runs out here. */
export const END = 49;

/**
 * The recording's own clock, as the beat a composition frame falls on.
 *
 * It was captured at about 24 frames a second with a screen recorder's jitter
 * and this plays at 30, so the two only line up every fifth frame. Every table
 * below is in *beats* rather than frames for that reason: it is the only way a
 * number measured off frame 23 of the recording can be the number this scene is
 * on when frame 23 of the recording is what it is being compared to.
 */
export const BEATS = 39;
export const BEAT: readonly Pt[] = [
  [0, 0],
  [1, 1],
  [2, 2],
  [3, 3],
  [4, 4],
  [6, 5],
  [7, 6],
  [8, 7],
  [9, 8],
  [10, 9],
  [12, 10],
  [13, 11],
  [14, 12],
  [15, 13],
  [17, 14],
  [18, 15],
  [19, 16],
  [20, 17],
  [22, 18],
  [23, 19],
  [24, 20],
  [26, 21],
  [27, 22],
  [28, 23],
  [29, 24],
  [31, 25],
  [32, 26],
  [33, 27],
  [34, 28],
  [35, 29],
  [37, 30],
  [38, 31],
  [39, 32],
  [41, 33],
  [42, 34],
  [43, 35],
  [44, 36],
  [45, 37],
  [47, 38],
  [48, 39],
];

export type Pt = readonly [number, number];

/** Linear read of a `[key, value]` table, clamped at both ends. */
export function track(table: readonly Pt[], at: number): number {
  const first = table[0];
  const last = table[table.length - 1];
  if (!first || !last) return 0;
  if (at <= first[0]) return first[1];
  if (at >= last[0]) return last[1];
  for (let i = 0; i < table.length - 1; i++) {
    const a = table[i];
    const b = table[i + 1];
    if (!a || !b) break;
    if (at <= b[0]) {
      const t = (at - a[0]) / (b[0] - a[0]);
      return a[1] + (b[1] - a[1]) * t;
    }
  }
  return last[1];
}

/* ─────────────────────────────────────────────────────────────────────────
   Type
   ───────────────────────────────────────────────────────────────────────── */

/** Cap height and line spacing of the settled block, at scale 1. */
export const CAP = 35.82;
export const LINE_H = 48.18;
/**
 * How far the sentence block sits below where the lead word sat alone.
 *
 * Measured, and not a rounding error: the lead word's cap box is centred on the
 * card to within a tenth of a pixel, and the two-line block that replaces it
 * sits nearly six pixels lower than the same rule would put it. Left out, every
 * baseline in the scene is six pixels high.
 */
export const DROP = 5.77;
/** The face's cap height, which is what `CAP` is turned into a font size with. */
export const CAP_RATIO = 0.74909;
/** The face's own hhea metrics: at `lineHeight: LEADING` the baseline is here. */
export const ASC = 0.9527;
export const LEADING = 1.2103;
export const TRACKING = -0.055;
/**
 * And an extra trim on the space, on top of the tracking.
 *
 * The reference sets its words 0.15 of a cap height apart. The face's own space
 * is more than twice that, and tracking tight enough to close it would take the
 * letters with it — so the two are separate numbers, which is what a typesetter
 * would call word-spacing and letter-spacing and is exactly the distinction.
 */
export const SPACE_TRIM = -0.0549;

/* ─────────────────────────────────────────────────────────────────────────
   The clock
   ───────────────────────────────────────────────────────────────────────── */

/** The beat the second word drops in on, and the gap between the ones after. */
export const FIRST_AT = 20;
export const STAGGER = 1;
/** How many beats a word stays in the accent colour after it drops in. */
export const SETTLE = 4;
/** The lead word starts this far above its own resting place. */
export const LEAD_RISE = 20.6;

/**
 * How far a word has travelled from where it dropped in to its slot, against
 * beats since it dropped.
 *
 * It is an exponential settle — about 0.7 of the remaining distance left every
 * beat — and the five words that do it are within 0.07 of each other on the
 * first beat and 0.02 by the fourth, which is the capture's sub-frame jitter
 * and not five different curves.
 */
export const TRAVEL: readonly Pt[] = [
  [0, 0],
  [1, 0.391],
  [2, 0.563],
  [3, 0.686],
  [4, 0.77],
  [5, 0.836],
  [6, 0.887],
  [7, 0.928],
  [8, 0.96],
  [9, 0.982],
  [10, 0.991],
  [12, 0.996],
  [14, 1],
];

/**
 * The lead word sliding across the line, and dropping onto it.
 *
 * Two tables because the reference has two moves. `LEAD_X` is the line
 * reflowing — an ease *in* and out, twenty-one beats long, which is what a line
 * that has just been told it is about to get five more words does. `LEAD_Y` is
 * the block going from one line to two, and it is over in six.
 */
export const LEAD_X: readonly Pt[] = [
  [10, 0],
  [11, 0.004],
  [12, 0.007],
  [13, 0.02],
  [14, 0.036],
  [15, 0.063],
  [16, 0.107],
  [17, 0.18],
  [18, 0.317],
  [19, 0.547],
  [20, 0.701],
  [21, 0.793],
  [22, 0.852],
  [23, 0.892],
  [24, 0.921],
  [25, 0.946],
  [26, 0.959],
  [27, 0.97],
  [28, 0.98],
  [29, 0.99],
  [30, 0.992],
  [31, 1],
];

export const LEAD_Y: readonly Pt[] = [
  [0, 0],
  [1, 0.131],
  [2, 0.211],
  [3, 0.27],
  [4, 0.329],
  [5, 0.35],
  [6, 0.391],
  [7, 0.411],
  [8, 0.4341],
  [9, 0.47],
  [10, 0.514],
  [11, 0.612],
  [12, 0.817],
  [13, 0.905],
  [14, 0.949],
  [15, 0.985],
  [16, 1],
];

/**
 * The block's own scale.
 *
 * It holds at 1.289 while the words are still coming and then falls to 1 over
 * ten beats, with the whole middle of that fall inside two of them. The first
 * eighteen are flat because they are measured off one word's ink width and
 * wobble a third of a percent; the fall is every word at once and does not.
 */
export const SCALE: readonly Pt[] = [
  [0, 1.289],
  [18, 1.289],
  [19, 1.2798],
  [20, 1.2782],
  [21, 1.2727],
  [22, 1.264],
  [23, 1.2546],
  [24, 1.2414],
  [25, 1.2184],
  [26, 1.159],
  [27, 1.0689],
  [28, 1.0417],
  [29, 1.0275],
  [30, 1.0193],
  [31, 1.0132],
  [32, 1.0081],
  [33, 1.0064],
  [34, 1],
  [BEATS, 1],
];

/**
 * Where each word drops in, relative to its own slot, in card pixels.
 *
 * Measured off the frame each word first appears on, and indexed by the word's
 * place in the sentence rather than by the order it drops in. There is no
 * pattern in them and there is not meant to be — three of the five come in flat
 * from the side and two fall past their line — so they are a table and not a
 * formula, and a sentence longer than the reference's simply reuses them.
 */
export const SCATTER: readonly Pt[] = [
  [44.4, -59.2],
  [73.8, -1],
  [-31.7, -1.7],
  [12.4, 41],
  [96.2, -1.7],
];

/**
 * The order the words drop in, as offsets into the sentence.
 *
 * The last word first, then the third, the first, the fourth, the second — so
 * the sentence is never briefly readable and never briefly wrong, it is just
 * scattered until it is not.
 */
export const ORDER: readonly number[] = [5, 3, 1, 4, 2];

/**
 * The order `n` words after the lead drop in.
 *
 * The reference's own order where it fits, folded into range where it does not,
 * and then whatever is left in reading order. It has to be a permutation — one
 * repeated index is a word that never drops and a hole in the sentence — which
 * is why it is built as a list and not as arithmetic.
 */
export function dropOrder(n: number): number[] {
  const out: number[] = [];
  const seen = new Set<number>();
  const take = (i: number) => {
    if (i >= 1 && i <= n && !seen.has(i)) {
      seen.add(i);
      out.push(i);
    }
  };
  for (const seed of ORDER) take(seed <= n ? seed : ((seed - 1) % n) + 1);
  for (let i = 1; i <= n; i++) take(i);
  return out;
}

/** Which word drops in `k`th. */
export function orderAt(k: number, n: number): number {
  return dropOrder(n)[k] ?? k + 1;
}

/* ─────────────────────────────────────────────────────────────────────────
   Layout
   ───────────────────────────────────────────────────────────────────────── */

const MEASURE_FALLBACK = 0.52;

/** Advance width of each word, from canvas, with an SSR-safe estimate. */
export function measureWords(
  words: string[],
  fontSize: number,
  weight: number,
  face: string,
): { widths: number[]; space: number } {
  const guess = {
    widths: words.map((w) => w.length * fontSize * MEASURE_FALLBACK),
    space: fontSize * 0.26,
  };
  if (typeof document === "undefined") return guess;
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) return guess;
  ctx.font = `${weight} ${fontSize}px ${face}`;
  return {
    widths: words.map((w) => ctx.measureText(w).width),
    space: ctx.measureText(" ").width,
  };
}

export type Slot = { x: number; baseline: number };

/**
 * The sentence, wrapped greedily and centred line by line.
 *
 * `maxWidth` is in card pixels at scale 1, so it is the same number whatever
 * the block's scale is doing — which is the only reason the line break holds
 * still while the block shrinks by a third.
 */
export function solve(
  widths: number[],
  space: number,
  maxWidth: number,
): Slot[] {
  const lines: number[][] = [[]];
  let run = 0;
  for (let i = 0; i < widths.length; i++) {
    const w = widths[i] ?? 0;
    const line = lines[lines.length - 1] as number[];
    const add = (line.length ? space : 0) + w;
    if (line.length > 0 && run + add > maxWidth) {
      lines.push([i]);
      run = w;
    } else {
      line.push(i);
      run += add;
    }
  }
  const top = CENTRE_Y + CAP / 2 + DROP - ((lines.length - 1) * LINE_H) / 2;
  const out: Slot[] = [];
  lines.forEach((line, k) => {
    let total = 0;
    for (const i of line) total += widths[i] ?? 0;
    total += space * (line.length - 1);
    let x = LINE_CX - total / 2;
    for (const i of line) {
      out[i] = { x, baseline: top + k * LINE_H };
      x += (widths[i] ?? 0) + space;
    }
  });
  return out;
}

/* ─────────────────────────────────────────────────────────────────────────
   The component
   ───────────────────────────────────────────────────────────────────────── */

export interface WordGatherProps {
  /** The sentence. The first word is the one that holds the frame alone. */
  text?: string;
  /**
   * Where the line breaks, in card pixels at scale 1. Wider is fewer lines;
   * the reference breaks a six-word sentence into two.
   */
  maxWidth?: number;
  /** The colour a word wears while it is still travelling. */
  accent?: string;
  fontWeight?: number;
  fontFamily?: string;
  theme?: Partial<SnapCnTheme>;
  mode?: "light" | "dark";
  speed?: number;
  className?: string;
}

export function WordGather({
  text = "Every scene is yours to own.",
  maxWidth = 330,
  accent,
  fontWeight = 500,
  fontFamily,
  theme,
  mode = "light",
  speed = 1,
  className,
}: WordGatherProps) {
  const frame = useCurrentFrame() * speed;
  const { width, height } = useVideoConfig();
  const t = useSnapCnTheme(theme, mode);
  const face = resolveFont(fontFamily ?? t.fontFamily) ?? SANS;
  const ink = t.foreground;
  // The customiser hands an unset colour control through as "", not undefined.
  const lit = accent || t.primary;

  // Carrying each word's place in the sentence as its own id, because a
  // sentence can say the same word twice and the pair is what identifies it.
  const items = useMemo(
    () =>
      text
        .split(/\s+/)
        .filter(Boolean)
        .map((word, at) => ({ word, at, id: `${at}:${word}` })),
    [text],
  );
  const words = useMemo(() => items.map((w) => w.word), [items]);
  const size = CAP / CAP_RATIO;

  // Canvas measurement is not available during SSR, so the first client render
  // has to use the same estimate the server used or React logs a hydration
  // mismatch; the precise widths land on the next tick.
  const [ready, setReady] = useState(false);
  useEffect(() => setReady(true), []);
  const { widths, space } = useMemo(() => {
    const raw = ready
      ? measureWords(words, size, fontWeight, face)
      : {
          widths: words.map((w) => w.length * size * MEASURE_FALLBACK),
          space: size * 0.26,
        };
    // `letter-spacing` lands after every character, the last one included, so a
    // word's advance grows by its whole length and the space by one.
    return {
      widths: raw.widths.map(
        (w, i) => w + (words[i]?.length ?? 0) * TRACKING * size,
      ),
      space: raw.space + (TRACKING + SPACE_TRIM) * size,
    };
  }, [ready, words, size, fontWeight, face]);
  const slots = useMemo(
    () => solve(widths, space, maxWidth),
    [widths, space, maxWidth],
  );

  // Everything below is on the recording's clock, not the composition's.
  const b = track(BEAT, frame);
  const s = track(SCALE, b);
  const lead = slots[0];
  const aloneX = LINE_CX - (widths[0] ?? 0) / 2;
  const aloneBase = CENTRE_Y + CAP / 2;
  const lx = track(LEAD_X, b);
  const ly = track(LEAD_Y, b);

  // `cover`, so a 16:9 composition crops the card rather than letterboxing it.
  const k = Math.max(width / REF_W, height / REF_H);

  return (
    <AbsoluteFill
      className={className}
      style={{ background: t.background, overflow: "hidden" }}
    >
      <AbsoluteFill
        style={{
          width: REF_W,
          height: REF_H,
          left: (width - REF_W * k) / 2,
          top: (height - REF_H * k) / 2,
          transform: `scale(${k})`,
          transformOrigin: "top left",
          background: t.background,
          borderRadius: RADIUS,
          overflow: "hidden",
          fontFamily: face,
          textRendering: "geometricPrecision",
        }}
      >
        {items.map(({ word, at: i, id }) => {
          const slot = slots[i];
          if (!slot) return null;
          let x = slot.x;
          let base = slot.baseline;
          let colour = ink;
          if (i === 0 && lead) {
            x = slot.x + (aloneX - slot.x) * (1 - lx);
            base =
              slot.baseline +
              (aloneBase - slot.baseline + LEAD_RISE) * (1 - ly);
          } else {
            const k2 = dropSlot(i, words.length - 1);
            const at = FIRST_AT + STAGGER * k2;
            if (b < at) return null;
            const p = track(TRAVEL, b - at);
            const d = SCATTER[(i - 1) % SCATTER.length] ?? [0, 0];
            x = slot.x + d[0] * (1 - p);
            base = slot.baseline + d[1] * (1 - p);
            if (b < at + SETTLE) colour = lit;
          }
          return (
            <Word
              key={id}
              text={word}
              x={CENTRE_X + (x - CENTRE_X) * s}
              baseline={CENTRE_Y + (base - CENTRE_Y) * s}
              size={size}
              scale={s}
              weight={fontWeight}
              colour={colour}
            />
          );
        })}
      </AbsoluteFill>
    </AbsoluteFill>
  );
}

/** Which drop-in slot word `i` of `n` is on. */
function dropSlot(i: number, n: number): number {
  const k = dropOrder(n).indexOf(i);
  return k < 0 ? i - 1 : k;
}

/* ─────────────────────────────────────────────────────────────────────────
   One word, scaled about its own baseline
   ───────────────────────────────────────────────────────────────────────── */

function Word({
  text,
  x,
  baseline,
  size,
  scale,
  weight,
  colour,
}: {
  text: string;
  x: number;
  baseline: number;
  size: number;
  scale: number;
  weight: number;
  colour: string;
}) {
  const rise = ASC * size;
  return (
    <div
      style={{
        position: "absolute",
        left: x,
        top: baseline - rise,
        width: "max-content",
      }}
    >
      <div
        style={{
          transform: `scale(${scale})`,
          transformOrigin: `0 ${rise}px`,
          fontSize: size,
          lineHeight: LEADING,
          fontWeight: weight,
          letterSpacing: `${TRACKING}em`,
          color: colour,
          whiteSpace: "pre",
        }}
      >
        {text}
      </div>
    </div>
  );
}
