import React from "react";
import { SANS } from "../theme";

/** parley's on-screen pill (src/overlay.rs), drawn the same way: the dot, 24 level
 *  bars, the timer, the text and the dim detail. `scale` multiplies its 13 px type. */
export type PillState = "rec" | "answer" | "work" | "speak" | "done" | "error" | "info";

const BARS = 24;

export const Pill: React.FC<{
  state: PillState;
  text: string;
  detail?: string;
  dim?: boolean;
  levels?: number[];
  tick: number; // the overlay's frame counter (it redraws at ~30 Hz)
  seconds?: number;
  scale?: number;
  style?: React.CSSProperties;
}> = ({ state, text, detail, dim, levels = [], tick, seconds = 0, scale = 1, style }) => {
  const recording = state === "rec" || state === "answer";
  const meter = recording || state === "work" || state === "speak";
  const note = state === "done" || state === "error" || state === "info";
  const h = 24;
  const mid = h / 2;
  const [dot, bar] =
    state === "rec"
      ? ["rgb(255,77,94)", "rgb(237,237,237)"]
      : state === "answer"
        ? ["rgb(77,163,255)", "rgb(237,237,237)"]
        : ["rgb(158,140,255)", "rgb(158,140,255)"];
  const lv = Array.from({ length: BARS }, (_, i) => levels[levels.length - BARS + i] ?? 0);
  const speaking = lv.slice(-3).some((l) => l > 0.35);
  const r = recording ? (speaking ? 5.5 : 4.5 + Math.abs(Math.sin(tick / 6))) : 4.5;
  const mark = state === "done" ? "✓" : state === "error" ? "!" : "";
  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 10,
        background: "rgba(22, 22, 26, 0.94)",
        border: "1px solid rgba(255, 255, 255, 0.09)",
        borderRadius: 21,
        padding: "7px 16px 7px 13px",
        color: "#ececec",
        fontFamily: SANS,
        fontSize: 13,
        whiteSpace: "nowrap",
        boxShadow: "0 10px 30px -10px rgba(0,0,0,0.35)",
        zoom: scale,
        ...style,
      }}
    >
      {meter && (
        <svg width={14 + BARS * 5} height={h} style={{ flexShrink: 0 }}>
          <circle cx={6} cy={mid} r={r} fill={dot} />
          {Array.from({ length: BARS }, (_, i) => {
            const level = recording ? lv[i] : 0.25 + 0.2 * Math.sin(tick / 3 - i * 0.45);
            const bh = Math.min(2 + level * (h - 6), h - 2);
            return (
              <rect
                key={i}
                x={16 + i * 5}
                y={mid - bh / 2}
                width={3}
                height={bh}
                rx={1.5}
                fill={bar}
                opacity={level > 0.2 ? 1 : 0.45}
              />
            );
          })}
        </svg>
      )}
      {note && mark && (
        <span style={{ fontWeight: 700, color: state === "done" ? "#4ade80" : "#fbbf24" }}>{mark}</span>
      )}
      {recording && (
        <span style={{ fontFeatureSettings: "'tnum'", color: "rgba(236,236,236,0.55)", fontSize: 12 }}>
          {Math.floor(seconds / 60)}:{String(Math.floor(seconds % 60)).padStart(2, "0")}
        </span>
      )}
      <span
        style={{
          color: dim ? "rgba(236,236,236,0.55)" : "#ececec",
          maxWidth: "48ch",
          overflow: "hidden",
          textOverflow: "ellipsis",
          direction: recording ? "rtl" : "ltr", // the overlay ellipsizes the start while you talk
          textAlign: "left",
        }}
      >
        <bdi>{text}</bdi>
      </span>
      {detail && (state === "work" || state === "speak") && (
        <span
          style={{
            color: "rgba(236,236,236,0.55)",
            maxWidth: "28ch",
            overflow: "hidden",
            textOverflow: "ellipsis",
            direction: state === "work" ? "rtl" : "ltr",
          }}
        >
          <bdi>{detail}</bdi>
        </span>
      )}
    </div>
  );
};
