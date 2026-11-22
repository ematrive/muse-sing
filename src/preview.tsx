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
    start(req: { parts: { name: string }[]; cache_only?: boolean }) {
      const me = ++job;
      if (req.cache_only) {
        // reopening: ready again with ?cached=1, otherwise nothing in the cache
        const hit = !!new URLSearchParams(location.search).get("cached");
        setTimeout(() => emit("sing-done", { job: me, error: null, cancelled: !hit, warnings: [], seconds: hit ? total : 0 }), 600);
        return me;
      }
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

// --- fake projects: built from the analysis of the preview score -----------------------------------
type PAnalysis = { title: string; file_name: string; lang: string; duration_sec: number; parts: { name: string; sung: boolean; notes: number }[] };
function opened(path: string | null) {
  const an = (q.get("song") === "sonnet" ? sonnet : santo) as PAnalysis | undefined;
  if (!an) throw "Anteprima senza partiture: mancano src/preview-santo.json e src/preview-sonnet.json";
  const parts = an.parts
    .filter((p) => p.sung)
    .map((p) => ({ name: p.name, sing: true, singer: "", color: null, volume_db: 0, pan: 0, mute: false, solo: false, voices: 1, played: false }));
  return {
    project: {
      version: 1,
      title: an.title,
      score_name: an.file_name,
      lang: an.lang,
      pitch: "model",
      tempo_factor: 1,
      parts,
      accompaniment: { enabled: an.parts.some((p) => !p.sung), volume_db: -6, pan: 0, mute: false, solo: false },
      edits: {},
      ambience: "chiesa",
      reverb: 0.5,
      instruments: {},
      metronome: { enabled: false, volume_db: -6, pan: 0, mute: false, solo: false },
      count_in: false,
      marks: true,
      transpose: 0,
      guides: "evidenza",
    },
    path,
    score_path: "/tmp/cache/projects/0123.mscz",
    analysis: an,
    default_dir: "/Users/utente/Music",
  };
}
// 120 bpm at 100%: 960 ticks a second
let rollTempo = 1;
function roll(tempoFactor = 1) {
  rollTempo = tempoFactor;
  const an = (q.get("song") === "sonnet" ? sonnet : santo) as PAnalysis;
  const syl = ["san", "to", "si", "gno", "re", "dio", "del", "l'u", "ni", "ver", "so"];
  const parts = an.parts.map((p, i) => {
    const notes = [];
    let t = 0;
    const dur = q.get("long") ? 240 : an.duration_sec;
    for (let k = 0; t < dur; k++) {
      const d = [0.5, 1, 1.5, 0.75][(k + i) % 4];
      const tick = Math.round(t * 960);
      const endTick = Math.round((t + d * 0.95) * 960);
      notes.push({
        id: `${k + 1}.1.0`,
        tick,
        end_tick: endTick,
        start: tick / 960 / tempoFactor,
        end: endTick / 960 / tempoFactor,
        pitch: 72 - i * 6 + ((k * 7 + i * 3) % 9) - 4,
        lyric: p.sung ? syl[k % syl.length] : null,
      });
      t += d;
    }
    return { name: p.name, sung: p.sung, notes };
  });
  // 4/4 at 120 (1920 ticks a measure), with a 3/4 passage from measure 9 to 12
  const dur = q.get("long") ? 240 : an.duration_sec;
  const bars = [];
  for (let n = 1, tick = 0; tick / 960 < dur; n++) {
    const sig = n === 9 ? "3/4" : n === 13 || n === 1 ? "4/4" : null;
    bars.push({ tick, start: tick / 960 / tempoFactor, number: String(n), time_sig: sig });
    tick += n >= 9 && n <= 12 ? 1440 : 1920;
  }
  return { parts, bars, tempos: [[0, 120 * tempoFactor]] };
}
function trackInfo(part: string) {
  const p = roll(rollTempo).parts.find((x) => x.name === part);
  if (!p) return null;
  const words = p.notes.map((n) => {
    const syl = n.lyric ?? "a";
    const cons = syl.replace(/[aeiouà]/g, "").split("").slice(0, 2);
    const vowel = syl.match(/[aeiouà]/)?.[0] ?? "a";
    const sym = [...cons.map((c) => (c === "'" ? "l" : c)), vowel];
    const start = n.start - 0.06 * cons.length;
    const phones = sym.map((s, i) => {
      const a = i < cons.length ? start + 0.06 * i : n.start;
      const b = i + 1 < cons.length ? a + 0.06 : i < cons.length ? n.start : n.end;
      return { symbol: s, start: a, end: b, base: a };
    });
    return { id: n.id, word: syl.replace("'", ""), start: n.start, end: n.end, phones };
  });
  const step = 0.0116;
  const pitch = p.notes.map((n) => ({
    start: n.start,
    step,
    midi: Array.from({ length: Math.floor((n.end - n.start) / step) }, (_, i) => n.pitch + 0.25 * Math.sin(i * step * 2 * Math.PI * 5.5) * Math.min(1, i / 40)),
  }));
  const phonemes = ["a", "e", "i", "o", "u", "b", "d", "g", "k", "l", "m", "n", "p", "r", "s", "t", "v", "z", "ny", "gl", "ch"].map((s) => [s, "aeiou".includes(s)]);
  return { words, pitch, phonemes, gender: true, velocity: true };
}

// ?mute=1: a microphone that sends nothing (permission missing)
setInterval(() => emit("audio-level", q.get("mute") ? 0 : 0.05 + Math.random() * 0.25), 50);
// for UI checks: inject events and look at the commands sent
const w = window as unknown as { __emit: typeof emit; __calls: [string, unknown][]; __cacheCleared?: boolean };
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
    if (cmd === "soundfont_installed") return q.get("sf") === "1";
    if (cmd === "install_soundfont") return "/tmp/GeneralUser-GS.sf2";
    if (cmd === "install_tiger") return "/Users/utente/Library/Application Support/app.musesing.desktop/Singers/Tiger";
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
    if (cmd === "sing_start") return sing.start(a.req as unknown as { parts: { name: string }[]; cache_only?: boolean });
    if (cmd === "sing_cancel") return sing.cancel();
    if (cmd === "sing_play") return sing.play(a.fromSec);
    if (cmd === "sing_check" || cmd === "sing_check_replay") {
      // a pretend take: 2 s of singing, then a result made up from the part's notes
      const me = sing.play(Number(a.fromSec) || 0);
      if (cmd === "sing_check") {
        const from = Number(a.fromSec) || 0;
        const t0 = performance.now();
        const timer = window.setInterval(() => {
          const t = (performance.now() - t0) / 1000;
          const analyzing = t > (q.get("mute") ? 5 : 1.6);
          const level = analyzing || q.get("mute") ? 0 : 0.2 + 0.4 * Math.random();
          emit("sing-check", { job: me, t: from + Math.max(0, t - 0.8), counting: t < 0.8, analyzing, level });
          if (analyzing) clearInterval(timer);
        }, 50);
        const part = roll(rollTempo).parts.find((p) => p.name === a.part)!;
        window.setTimeout(() => {
          sing.stop();
          const inside = part.notes.filter((n) => n.start >= from && n.end <= from + 12);
          const verdicts = ["in_tune", "in_tune", "in_tune", "almost", "in_tune", "off", "in_tune", "missed"];
          emit("sing-check-done", {
            job: me,
            part: a.part,
            notes: inside.map((n, k) => ({ id: n.id, verdict: verdicts[k % verdicts.length], cents: [3, -8, 12, 38, -5, -70, 9, null][k % 8] })),
            line: inside.flatMap((n, k) => (k % 8 === 7 ? [] : Array.from({ length: Math.floor((n.end - n.start) / 0.02) }, (_, i) => [n.start + i * 0.02, n.pitch + [0.03, -0.08, 0.12, 0.38, -0.05, -0.7, 0.09, 0][k % 8] + 0.1 * Math.sin(i / 3)]))),
            range: [from, from + 12],
            lag: 0.03,
            error: null,
          });
        }, q.get("mute") ? 5600 : 2000);
      }
      return me;
    }
    if (cmd === "sing_export_guides") return ["/Users/utente/Music/Santo - Soprano.mp3", "/Users/utente/Music/Santo - Contralto.mp3"];
    if (cmd === "project_new" || cmd === "project_open") return opened(cmd === "project_open" ? "/Users/utente/Music/Santo.msng" : null);
    if (cmd === "project_save") return String(a.path).endsWith(".msng") ? a.path : `${a.path}.msng`;
    if (cmd === "project_reload_score") return opened(null);
    if (cmd === "project_sizes")
      return q.get("cached")
        ? { total: 31_400_000, score: 48_210, settings: 3_900, audio: 31_340_000, audio_parts: ["Soprano", "Contralto", "Baritono"] }
        : { total: 52_300, score: 48_210, settings: 3_900, audio: 0, audio_parts: [] };
    if (cmd === "cache_size") return w.__cacheCleared ? 0 : 734_000_000;
    if (cmd === "cache_clear") return void (w.__cacheCleared = true);
    if (cmd === "project_recent") return [{ path: "/Users/utente/Music/Santo.msng", title: "Santo" }];
    if (cmd === "take_opened_files") return [];
    if (cmd === "score_roll") return roll(Number(a.tempoFactor) || 1);
    if (cmd === "sing_track_info") return trackInfo(String(a.part));
    if (cmd === "plugin:dialog|ask") return true;
    if (cmd === "sing_stop") return sing.stop();
    if (cmd === "sing_export") return a.path;
    if (cmd === "plugin:dialog|open") return "/Users/utente/Music/Santo.mscz";
    if (cmd === "plugin:dialog|save") return "/Users/utente/Music/Santo.msng";
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
