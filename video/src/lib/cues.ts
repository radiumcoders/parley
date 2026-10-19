// The whole timeline in one place: each scene's length and, inside it, when each
// Gradium clip starts (scene-relative frames). Scene starts, the audio layer and the
// burned-in captions are all derived from this, so a re-voiced line re-times itself.
import { clipFrames as L, words } from "./timing";

/** Frames two neighbouring scenes overlap during a slide. */
export const T = 18;

const hook = (() => {
  const p1 = 12;
  const p2 = p1 + L("n_problem1") + 14;
  const p3 = p2 + L("n_problem2") + 6;
  return { p1, p2, p3, dur: p3 + L("n_problem3") + 26 };
})();

const positioning = (() => {
  const intro = 14;
  const how = intro + L("n_intro") + 10;
  return { intro, how, dur: how + L("n_how") + 34 };
})();

const reveal = (() => {
  const press = 24;
  const speak = press + 10;
  const release = speak + L("d_before") + 10;
  const transcribing = release + 2;
  const cleaning = release + 16;
  const done = release + 52;
  const enter = done + 12;
  return { press, speak, release, transcribing, cleaning, done, enter, dur: enter + 66 };
})();

const beforeAfter = (() => {
  const gets = 12;
  const after = gets + L("n_gets") + 10;
  const compare = after + L("d_after") + 12;
  return { gets, after, compare, dur: compare + L("n_compare") + 34 };
})();

const askBack = (() => {
  const question = 10;
  const ask = 18;
  const chime = ask + Math.round(2.16 * 30); // on "chimes"
  const reply = ask + L("f_ask") + 4;
  const replyEnd = reply + L("d_reply") + 8;
  const cleaning = replyEnd + 2;
  const done = replyEnd + 30;
  const enter = done + 10;
  const answerLine = enter + 14;
  const speak = answerLine + L("f_answer") + 6;
  return {
    question, ask, chime, reply, replyEnd, cleaning, done, enter, answerLine, speak,
    dur: speak + L("c_answer") + 26,
  };
})();

const commands = (() => {
  const line = 12;
  return { line, dur: line + L("f_commands") + 30 };
})();

const languages = (() => {
  const line = 10;
  const speak = line + L("f_lang") + 14;
  const release = speak + L("d_french") + 8;
  const cleaning = release + 2;
  const done = release + 30;
  return { line, speak, release, cleaning, done, dur: done + 64 };
})();

const learn = (() => {
  const line = 10;
  const history = line + L("f_learn") + 16;
  return { line, history, dur: history + L("f_history") + 30 };
})();

const outro = (() => {
  const line = 18;
  return { line, dur: line + L("n_outro") + 60 };
})();

export const S = { hook, positioning, reveal, beforeAfter, askBack, commands, languages, learn, outro };
export type SceneKey = keyof typeof S;
export const ORDER: SceneKey[] = [
  "hook", "positioning", "reveal", "beforeAfter", "askBack", "commands", "languages", "learn", "outro",
];

/** Absolute start frame of each scene, accounting for the slide overlaps. */
export const START = (() => {
  const out = {} as Record<SceneKey, number>;
  let at = 0;
  for (const k of ORDER) {
    out[k] = at;
    at += S[k].dur - T;
  }
  return out;
})();

export const TOTAL = ORDER.reduce((n, k) => n + S[k].dur, 0) - T * (ORDER.length - 1);

/** Every sound, at its absolute frame. */
export const AUDIO: { src: string; at: number; volume?: number }[] = [
  { src: "n_problem1", at: START.hook + hook.p1 },
  { src: "n_problem2", at: START.hook + hook.p2 },
  { src: "n_problem3", at: START.hook + hook.p3 },
  { src: "n_intro", at: START.positioning + positioning.intro },
  { src: "n_how", at: START.positioning + positioning.how },
  { src: "chime_listen", at: START.reveal + reveal.press + 2 },
  { src: "d_before", at: START.reveal + reveal.speak },
  { src: "chime_done", at: START.reveal + reveal.done },
  { src: "n_gets", at: START.beforeAfter + beforeAfter.gets },
  { src: "d_after", at: START.beforeAfter + beforeAfter.after },
  { src: "n_compare", at: START.beforeAfter + beforeAfter.compare },
  { src: "f_ask", at: START.askBack + askBack.ask },
  { src: "chime_ask", at: START.askBack + askBack.chime },
  { src: "d_reply", at: START.askBack + askBack.reply },
  { src: "chime_done", at: START.askBack + askBack.done },
  { src: "f_answer", at: START.askBack + askBack.answerLine },
  { src: "c_answer", at: START.askBack + askBack.speak },
  { src: "f_commands", at: START.commands + commands.line },
  { src: "f_lang", at: START.languages + languages.line },
  { src: "chime_listen", at: START.languages + languages.speak - 8 },
  { src: "d_french", at: START.languages + languages.speak },
  { src: "chime_done", at: START.languages + languages.done },
  { src: "f_learn", at: START.learn + learn.line },
  { src: "f_history", at: START.learn + learn.history },
  { src: "n_outro", at: START.outro + outro.line },
];

/** The narrator's words, for the burned-in captions (absolute frames). */
const NARRATED = new Set([
  "n_problem1", "n_problem2", "n_problem3", "n_intro", "n_how", "n_gets", "n_compare",
  "f_ask", "f_answer", "f_commands", "f_lang", "f_learn", "f_history", "n_outro",
]);
export const CAPTIONS = AUDIO.filter((a) => NARRATED.has(a.src)).flatMap((a) =>
  words(a.src).map((w, i, all) => ({
    clip: a.src,
    text: w.text,
    startFrame: Math.round(a.at + w.at),
    endFrame: Math.round(a.at + (all[i + 1]?.at ?? L(a.src))),
  })),
);
