// Dev-only preview: the real UI with the Tauri backend mocked (data from the real Rust analysis).
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
// analyses of the choir scores: local only, not in the repository (see .gitignore)
const scores = import.meta.glob<{ default: unknown }>("./preview-{santo,sonnet}.json", { eager: true });
const santo = scores["./preview-santo.json"]?.default;
const sonnet = scores["./preview-sonnet.json"]?.default;
import voice from "./preview-voice.json";

const q = new URLSearchParams(location.search);
if (q.get("dark")) document.documentElement.classList.add("dark");
mockWindows("main");

// --- a fake audio engine: progress events in real time, like the Rust one -------------------------
type Phr = { id: string; bpm: number; notes: { beats: number }[] };
const phrases = (voice as unknown as { phrases: Phr[] }).phrases;
let job = 0;
let timer: number | undefined;
const devices = {
  inputs: [
    { name: "AirPods Pro", bluetooth: true, default: true },
    { name: "Microfono MacBook Air", bluetooth: false, default: false },
  ],
  outputs: [
    { name: "AirPods Pro", bluetooth: true, default: true },
    { name: "Altoparlanti MacBook Air", bluetooth: false, default: false },
  ],
};

function run(kind: "guide" | "take" | "play", id: string, from: number, speed = 1) {
  if (timer) clearInterval(timer);
  const me = ++job;
  const ph = phrases.find((p) => p.id === id)!;
  const beat = 60 / (ph.bpm * speed);
  const beats = ph.notes.reduce((s, n) => s + n.beats, 0);
  const countIn = kind === "take" ? 4 * beat : 0;
  const total = countIn + beats * beat + (kind === "take" ? 1 : 0.3);
  const t0 = performance.now() + (kind === "take" ? 300 : 0);
  timer = window.setInterval(() => {
    const t = (performance.now() - t0) / 1000;
    if (t >= total - from * beat) {
      clearInterval(timer);
      timer = undefined;
      const take = kind === "take" ? { file: `wavs/${id}.wav`, seconds: 4.2, peak: 0.4, clipped: false, quiet: false, sample_rate: 48000, recorded_at: 0 } : null;
      emit("audio-done", { job: me, kind, id, take, error: null, cancelled: false });
      return;
    }
    const phase = t < 0 ? "wait" : t < countIn ? "countin" : t < countIn + beats * beat ? "sing" : "tail";
    const pos = kind === "take" ? (t - countIn) / beat : from + Math.max(0, t) / beat;
    emit("audio-progress", { job: me, kind, phase, t, total, beats: pos, level: 0.3 });
  }, 30);
  return me;
}

// --- a fake render / playback of the sung mix ---------------------------------------------------
const sing = (() => {
  let job = 100;
  let renderTimer: number | undefined;
  let playTimer: number | undefined;
  const total = 154;
  return {
    start(req: { parts: { name: string }[] }) {
      const me = ++job;
      let i = 0;
      const per = 6;
      clearInterval(renderTimer);
      renderTimer = window.setInterval(() => {
        const index = Math.floor(i / per);
        if (index >= req.parts.length) {
          clearInterval(renderTimer);
          emit("sing-done", { job: me, error: null, cancelled: false, warnings: ["Basso: «signore»: fonemi non riconosciuti JJ"], seconds: total });
          return;
        }
        emit("sing-progress", { job: me, part: req.parts[index].name, index, parts: req.parts.length, phrase: i % per, phrases: per, cached: false });
        i++;
      }, 250);
      return me;
    },
    cancel() {
      const me = job;
      setTimeout(() => {
        clearInterval(renderTimer);
        emit("sing-done", { job: me, error: null, cancelled: true, warnings: [], seconds: 0 });
      }, 800);
    },
    play(from: number) {
      const me = ++job;
      clearInterval(playTimer);
      const t0 = performance.now();
      playTimer = window.setInterval(() => {
        const t = from + (performance.now() - t0) / 1000;
        if (t >= total) {
          clearInterval(playTimer);
          emit("sing-play-done", { job: me, error: null });
          return;
        }
        emit("sing-play", { job: me, t, total });
      }, 50);
      return me;
    },
    stop() {
      clearInterval(playTimer);
      emit("sing-play-done", { job, error: null });
    },
  };
})();

setInterval(() => emit("audio-level", 0.05 + Math.random() * 0.25), 50);
// for UI checks: inject events and look at the commands sent
const w = window as unknown as { __emit: typeof emit; __calls: [string, unknown][] };
w.__emit = emit;
w.__calls = [];

mockIPC(
  (cmd, args) => {
    const a = (args ?? {}) as Record<string, never>;
    if (cmd.startsWith("audio_")) w.__calls.push([cmd, args]);
    if (cmd === "analyze") {
      const a = q.get("song") === "sonnet" ? sonnet : santo;
      if (!a) throw "Anteprima senza partiture: mancano src/preview-santo.json e src/preview-sonnet.json";
      return a;
    }
    if (cmd === "list_singers")
      return {
        singers: [
          {
            id: "TIGER_DS_v106",
            name: "TIGER DS v106",
            singer_type: "diffsinger",
            langs: ["en", "es", "fr", "it", "ja"],
            path: "/Users/utente/Library/OpenUtau/Singers/TIGER_DS_v106",
            openutau: true,
            engine: true,
            engine_note: null,
            colors: ["fresh", "soft", "power", "falsetto"],
          },
        ],
        folders: ["/Users/utente/Library/OpenUtau/Singers"],
        own_folder: "/Users/utente/Library/Application Support/app.musesing.desktop/Singers",
        selected: { it: "TIGER_DS_v106", en: "TIGER_DS_v106" },
        parts: {},
      };
    if (cmd.startsWith("sing_")) w.__calls.push([cmd, args]);
    if (cmd === "sing_start") return sing.start(a.req as unknown as { parts: { name: string }[] });
    if (cmd === "sing_cancel") return sing.cancel();
    if (cmd === "sing_play") return sing.play(a.fromSec);
    if (cmd === "sing_stop") return sing.stop();
    if (cmd === "sing_export") return a.path;
    if (cmd === "generate") return { output: "/Users/utente/Music/Santo.ustx", tracks: 3, notes: 275 };
    if (cmd === "plugin:dialog|open") return "/Users/utente/Music/Santo.mscz";
    if (cmd === "plugin:dialog|save") return "/Users/utente/Music/Santo.ustx";
    if (cmd === "voice_recent") return [{ dir: voice.dir, name: voice.profile.name }];
    if (cmd === "voice_open" || cmd === "voice_create") return voice;
    if (cmd === "plugin:updater|check") return null;
    if (cmd === "audio_devices") return devices;
    if (cmd === "audio_open")
      return {
        input: a.input === "AirPods Pro" ? devices.inputs[0] : devices.inputs[1],
        output: devices.outputs[0],
        input_switched: !a.input,
        input_rate: 48000,
        output_rate: 48000,
      };
    if (cmd === "audio_guide") return run("guide", a.id, a.fromBeats, a.speed);
    if (cmd === "audio_record") return run("take", a.id, 0);
    if (cmd === "audio_play_take") return run("play", a.id, 0);
    if (cmd === "audio_stop") {
      if (timer) {
        clearInterval(timer);
        timer = undefined;
        emit("audio-done", { job, kind: "guide", id: "", take: null, error: null, cancelled: true });
      }
      return null;
    }
    if (cmd.startsWith("audio_")) return null;
    return null;
  },
  { shouldMockEvents: true },
);
await import("./main");
if (q.get("voice")) setTimeout(() => (document.querySelector("main button.btn") as HTMLButtonElement | null)?.click(), 300);
if (q.get("auto")) setTimeout(() => (document.querySelector("main button") as HTMLButtonElement | null)?.click(), 300);
