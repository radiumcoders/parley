import { loadFont as loadInter } from "@remotion/google-fonts/Inter";
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";

export const SANS = loadInter("normal", {
  weights: ["400", "500", "600", "700"],
  subsets: ["latin"],
}).fontFamily;
export const MONO = loadMono("normal", {
  weights: ["400", "500", "700"],
  subsets: ["latin"],
}).fontFamily;

export const C = {
  paper: "#F7F6F2",
  card: "#FFFFFF",
  line: "#E6E3DC",
  ink: "#141414",
  muted: "#6E6B66",
  faint: "#A9A59D",
  accent: "#6D5BF0", // parley's "working" purple, deepened for a light frame
  accentSoft: "#ECE9FF",
  rec: "#FF4D5E",
  answer: "#4DA3FF",
  ok: "#22B35E",
  warn: "#F59E0B",
  claude: "#D97757",
  filler: "#FFD8DC",
};

export const FPS = 30;
