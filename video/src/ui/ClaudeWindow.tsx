import React from "react";
import { interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { Mascot, THEMES } from "@/components/remocn/claude-code";
import { Caret } from "@/components/remocn/caret";
import { MONO } from "../theme";

// Claude Code in a terminal, after remocn's claude-code (its light palette and mascot),
// opened up so a scene can drive the transcript and the prompt from its own timeline.
const t = THEMES.light;
export const CLAUDE = t.boxBorder;

export const ClaudeWindow: React.FC<{
  prompt?: React.ReactNode;
  /** Shown dim when the prompt is empty. */
  placeholder?: string;
  children?: React.ReactNode; // the transcript, oldest first
  welcome?: boolean;
  width?: number;
  height?: number;
  enterAt?: number;
  style?: React.CSSProperties;
}> = ({ prompt, placeholder = 'Try "add rate limiting to <endpoint>"', children, welcome = true, width = 1040, height = 560, enterAt = 0, style }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({ fps, frame: frame - enterAt, config: { damping: 200 } });
  return (
    <div
      style={{
        width,
        height,
        background: t.windowBody,
        borderRadius: 14,
        overflow: "hidden",
        border: `1px solid ${t.windowBar}`,
        boxShadow: "0 1px 2px rgba(0,0,0,0.06), 0 12px 24px -12px rgba(0,0,0,0.18)",
        display: "flex",
        flexDirection: "column",
        fontFamily: MONO,
        color: t.fg,
        opacity: s,
        transform: `translateY(${interpolate(s, [0, 1], [24, 0])}px)`,
        ...style,
      }}
    >
      <div style={{ height: 34, background: t.windowBar, display: "flex", alignItems: "center", gap: 8, paddingLeft: 14, flexShrink: 0 }}>
        {["#FF5F57", "#FEBC2E", "#28C840"].map((c) => (
          <div key={c} style={{ width: 11, height: 11, borderRadius: "50%", background: c }} />
        ))}
        <div style={{ flex: 1, textAlign: "center", fontSize: 12, color: t.fgMuted, marginRight: 60 }}>~/code/api — claude</div>
      </div>
      <div style={{ flex: 1, padding: "20px 26px", display: "flex", flexDirection: "column", gap: 14, fontSize: 15, minHeight: 0 }}>
        {welcome && (
          <div style={{ border: `1px solid ${CLAUDE}`, borderRadius: 8, padding: "12px 16px", display: "flex", alignItems: "center", gap: 16, flexShrink: 0 }}>
            <Mascot accent={CLAUDE} size={40} />
            <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
              <div style={{ fontWeight: 700 }}>Claude Code <span style={{ color: t.fgMuted, fontWeight: 400 }}>v2.1.294</span></div>
              <div style={{ color: t.fgMuted, fontSize: 13 }}>~/code/api</div>
            </div>
          </div>
        )}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "flex-end", gap: 12, overflow: "hidden", minHeight: 0 }}>
          {children}
        </div>
        <div style={{ border: `1px solid ${t.fgDim}`, borderRadius: 8, padding: "10px 14px", display: "flex", alignItems: "flex-start", gap: 10, flexShrink: 0, minHeight: 22 }}>
          <span style={{ color: t.fgMuted }}>&gt;</span>
          {prompt ? (
            <span style={{ flex: 1, lineHeight: 1.45 }}>{prompt}</span>
          ) : (
            <span style={{ color: t.fgDim, display: "inline-flex", alignItems: "center" }}>
              <Caret width={9} height={18} color={t.fg} blink />
              <span style={{ marginLeft: 6 }}>{placeholder}</span>
            </span>
          )}
        </div>
      </div>
    </div>
  );
};

/** A prompt you sent, as Claude Code shows it in the transcript. */
export const UserTurn: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div style={{ display: "flex", gap: 10, background: "#F0EEE8", borderRadius: 6, padding: "8px 12px", lineHeight: 1.45 }}>
    <span style={{ color: t.fgMuted }}>&gt;</span>
    <span>{children}</span>
  </div>
);

/** Claude's reply or tool use: the ⏺ bullet and an optional ⎿ detail line. */
export const ClaudeTurn: React.FC<{ children: React.ReactNode; detail?: React.ReactNode; tool?: boolean }> = ({ children, detail, tool }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: 3, lineHeight: 1.45 }}>
    <div style={{ display: "flex", gap: 10 }}>
      <span style={{ color: tool ? "#22A06B" : t.fg }}>●</span>
      <span>{children}</span>
    </div>
    {detail && <div style={{ color: t.fgMuted, paddingLeft: 22 }}>⎿&nbsp; {detail}</div>}
  </div>
);

/** Claude's working line: a turning ✻ and a verb. */
export const Working: React.FC<{ verb?: string; from: number }> = ({ verb = "Thinking", from }) => {
  const frame = useCurrentFrame();
  const stars = ["·", "✢", "✳", "✶", "✻", "✽"];
  const star = stars[Math.floor(Math.max(0, frame - from) / 4) % stars.length];
  const secs = Math.max(0, Math.floor((frame - from) / 30));
  return (
    <div style={{ display: "flex", gap: 10, color: CLAUDE }}>
      <span style={{ width: 12 }}>{star}</span>
      <span>
        {verb}… <span style={{ color: t.fgMuted }}>({secs}s · esc to interrupt)</span>
      </span>
    </div>
  );
};

/** Only renders its children from `at` on, sliding them up a little. */
export const Appear: React.FC<{ at: number; children: React.ReactNode; style?: React.CSSProperties }> = ({ at, children, style }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (frame < at) return null;
  const s = spring({ fps, frame: frame - at, config: { damping: 200 }, durationInFrames: 12 });
  return <div style={{ opacity: s, transform: `translateY(${(1 - s) * 8}px)`, ...style }}>{children}</div>;
};
