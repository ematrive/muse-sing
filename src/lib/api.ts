import { invoke } from "@tauri-apps/api/core";

export type Lang = "it" | "en" | "la";

export type PartInfo = { name: string; notes: number; syllables: number; sung: boolean; preview: string };

export type Analysis = {
  path: string;
  file_name: string;
  title: string;
  lang: Lang;
  time_sig: string;
  bpm: number;
  duration_sec: number;
  parts: PartInfo[];
  warnings: string[];
  default_output: string;
};

export type SingerInfo = {
  id: string;
  name: string;
  singer_type: string;
  langs: string[];
  path: string;
  /** Installed in OpenUtau: usable in a .ustx. */
  openutau: boolean;
  /** Muse Sing's engine can sing with it; otherwise `engine_note` says why. */
  engine: boolean;
  engine_note: string | null;
  colors: string[];
};
export type PartPref = { singer: string; color: string | null };
export type SingersList = {
  singers: SingerInfo[];
  folders: string[];
  own_folder: string | null;
  selected: Partial<Record<Lang, string>>;
  /** Saved singer per part name (lowercase). */
  parts: Record<string, PartPref>;
};
export const api = {
  analyze: (path: string) => invoke<Analysis>("analyze", { path }),
  listSingers: () => invoke<SingersList>("list_singers"),
  soundfontInstalled: () => invoke<boolean>("soundfont_installed"),
  /** Downloads the instruments (SoundFont, about 31 MB); progress: `SOUNDFONT_EVENT`. */
  installSoundfont: () => invoke<string>("install_soundfont"),
  /** Downloads TIGER DS from its official release into Muse Sing's Singers folder. */
  installTiger: () => invoke<string>("install_tiger"),
};

/** Progress of `installTiger` (event `tiger-progress`); `total` is 0 when unknown. */
export type TigerProgress = { phase: "download" | "extract"; done: number; total: number };
export const TIGER_EVENT = "tiger-progress";
export const SOUNDFONT_EVENT = "soundfont-progress";

// --- projects (.msng) ----------------------------------------------------------------------

export type PartCfg = {
  name: string;
  sing: boolean;
  singer: string;
  color: string | null;
  volume_db: number;
  pan: number;
  mute: boolean;
  solo: boolean;
  /** singers in the section (1 = a soloist) */
  voices: number;
  /** played on the piano instead of sung */
  played: boolean;
};
export type Instrument = "organo_canne" | "armonium" | "pianoforte" | "archi" | "flauto" | "chitarra" | "arpa";
export const INSTRUMENTS: Record<Instrument, string> = {
  organo_canne: "Organo a canne",
  armonium: "Armonium",
  pianoforte: "Pianoforte",
  archi: "Archi",
  flauto: "Flauto",
  chitarra: "Chitarra",
  arpa: "Arpa",
};
/** The instrument guessed from a part name (as `Instrument::for_part` in Rust). */
export function instrumentFor(name: string): Instrument {
  const n = name.toLowerCase();
  const has = (keys: string[]) => keys.some((k) => n.includes(k));
  if (has(["armonium", "harmonium"])) return "armonium";
  if (has(["piano", "pf.", "tastier", "keyboard"])) return "pianoforte";
  if (has(["chitarr", "guitar"])) return "chitarra";
  if (has(["flaut", "flute"])) return "flauto";
  if (has(["viol", "archi", "string", "cello", "contrabb", "bass"]) && !has(["basso", "bassi"])) return "archi";
  if (has(["arpa", "harp"])) return "arpa";
  return "organo_canne";
}
export const METRONOME = "Metronomo";
export type Ambience = "nessuno" | "stanza" | "chiesa" | "cattedrale";
/** Room ambience of the mix, as sent to playback and export. */
export type Room = { ambience: Ambience; reverb: number };
export const AMBIENCES: Record<Ambience, string> = { nessuno: "Nessuno (secco)", stanza: "Stanza", chiesa: "Chiesa", cattedrale: "Cattedrale" };
export type AccompCfg = { enabled: boolean; volume_db: number; pan: number; mute: boolean; solo: boolean };
export type Project = {
  version: number;
  title: string;
  score_name: string;
  lang: Lang;
  pitch: PitchMode;
  tempo_factor: number;
  parts: PartCfg[];
  accompaniment: AccompCfg;
  edits: Record<string, PartEdits>;
  ambience: Ambience;
  /** amount of reverb 0..1 (0.5 = as designed) */
  reverb: number;
  instruments: Record<string, Instrument>;
  metronome: AccompCfg;
  count_in: boolean;
  /** follow dynamics, breaths, fermatas, rit./accel. of the score */
  marks: boolean;
  /** semitones, ±MAX_TRANSPOSE */
  transpose: number;
  guides: GuideKind;
};
export const MAX_TRANSPOSE = 6;
/** Study tracks: the part louder, the others without it, or both. */
export type GuideKind = "evidenza" | "senza" | "entrambe";
/** Curve points (tick, value): ticks of the written score, every CURVE_STEP; missing = default. */
export type Curve = [number, number][];
export const CURVE_STEP = 5;
/** `word`: the word when edited (an edit on a note that now sings another word is skipped). */
export type WordEdit = { word?: string; phonemes?: string | null; offsets?: number[] };
export type PartEdits = {
  words?: Record<string, string>;
  notes?: Record<string, WordEdit>;
  pitch?: Curve;
  dynamics?: Curve;
  gender?: Curve;
  velocity?: Curve;
};
/** A project as held by the UI: `path` null = never saved; `score_path` = working copy of the score. */
export type OpenedProject = { project: Project; path: string | null; score_path: string; analysis: Analysis; default_dir: string };
export type RecentProject = { path: string; title: string };
export type RollNote = { id: string; tick: number; end_tick: number; start: number; end: number; pitch: number; lyric: string | null };
export type RollPart = { name: string; sung: boolean; notes: RollNote[] };
/** `tempos`: (tick, bpm at the practice tempo). */
/** A measure as played: printed number (null for a pickup) and the time signature where it changes. */
export type RollBar = { tick: number; start: number; number: string | null; time_sig: string | null };
export type Roll = { parts: RollPart[]; bars: RollBar[]; tempos: [number, number][] };
/** Name of the accompaniment track in the mix (project.rs ACCOMPANIMENT). */
export const ACCOMPANIMENT = "Accompagnamento";

/** Extensions of the scores Muse Sing reads, and of its projects (`.musesing`: older versions). */
export const SCORE_EXTENSIONS = ["mscz", "mxl", "musicxml", "xml"];
export const PROJECT_EXTENSION = "msng";
export const PROJECT_EXTENSIONS = ["msng", "musesing"];
const extOf = (path: string) => path.split(".").pop()?.toLowerCase() ?? "";
export const isProjectPath = (path: string) => PROJECT_EXTENSIONS.includes(extOf(path));
export const isScorePath = (path: string) => SCORE_EXTENSIONS.includes(extOf(path));

export const projectApi = {
  /** New project from a score (MuseScore or MusicXML). */
  create: (score: string) => invoke<OpenedProject>("project_new", { score }),
  open: (path: string) => invoke<OpenedProject>("project_open", { path }),
  save: (path: string, project: Project, scorePath: string) => invoke<string>("project_save", { path, project, scorePath }),
  reloadScore: (project: Project, score: string) => invoke<OpenedProject>("project_reload_score", { project, score }),
  recent: () => invoke<RecentProject[]>("project_recent"),
  /** Files the Finder asked to open (double click) and not yet opened. */
  takeOpened: () => invoke<string[]>("take_opened_files"),
  roll: (scorePath: string, tempoFactor: number, marks: boolean, transpose: number) =>
    invoke<Roll>("score_roll", { scorePath, tempoFactor, marks, transpose }),
};

// --- sung audio (Muse Sing's DiffSinger engine) --------------------------------------------------

export type PitchMode = "model" | "notes";
export type RenderPart = { name: string; singer: string; color: string | null; voices: number; played: boolean };
export type RenderRequest = {
  path: string;
  lang: Lang;
  pitch: PitchMode;
  parts: RenderPart[];
  tempo_factor: number;
  accompaniment: boolean;
  edits: Record<string, PartEdits>;
  instruments: Record<string, Instrument>;
  marks: boolean;
  transpose: number;
  /** only read back the voices already in the cache (reopening a project) */
  cache_only?: boolean;
};
export type PhoneInfo = { symbol: string; start: number; end: number; base: number };
export type WordInfo = { id: string; word: string; start: number; end: number; phones: PhoneInfo[] };
export type PitchSeg = { start: number; step: number; midi: number[] };
/** What the editor shows of a rendered part (seconds at the practice tempo). */
export type TrackInfo = { words: WordInfo[]; pitch: PitchSeg[]; phonemes: [string, boolean][]; gender: boolean; velocity: boolean };
export type MixPart = { name: string; volume_db: number; pan: number; mute: boolean; solo: boolean };
export type SingProgress = { job: number; part: string; index: number; parts: number; phrase: number; phrases: number; cached: boolean };
export type SingDone = { job: number; error: string | null; cancelled: boolean; warnings: string[]; seconds: number };
export type PlayProgress = { job: number; t: number; total: number };
export type PlayDone = { job: number; error: string | null };
/** «Canta e controlla»: `sing-check` while singing, `sing-check-done` at the end. */
export type CheckProgress = { job: number; t: number; counting: boolean; analyzing: boolean; level: number };
export type Verdict = "in_tune" | "almost" | "off" | "missed" | "not_judged";
export type NoteCheck = { id: string; verdict: Verdict; cents: number | null };
export type CheckDone = {
  job: number;
  part: string;
  notes: NoteCheck[];
  /** sung pitch: (seconds, MIDI), next to the notes */
  line: [number, number][];
  range: [number, number];
  lag: number;
  /** "" = replaced by something else */
  error: string | null;
};

/** Render: `sing-progress` / `sing-done` events; playback: `sing-play` / `sing-play-done`. */
export const singApi = {
  start: (req: RenderRequest) => invoke<number>("sing_start", { req }),
  cancel: () => invoke<void>("sing_cancel"),
  trackInfo: (part: string) => invoke<TrackInfo | null>("sing_track_info", { part }),
  play: (mix: MixPart[], fromSec: number, loopRange: [number, number] | null, room: Room, countIn: boolean) =>
    invoke<number>("sing_play", { mix, fromSec, loopRange, ...room, countIn }),
  stop: () => invoke<void>("sing_stop"),
  export: (mix: MixPart[], path: string, room: Room) => invoke<string>("sing_export", { mix, path, ...room }),
  /** One file per sung part, that part in front and the others lowered by `othersDb`. */
  exportGuides: (mix: MixPart[], dir: string, ext: string, othersDb: number, room: Room, countIn: boolean, kind: GuideKind) =>
    invoke<string[]>("sing_export_guides", { mix, dir, ext, othersDb, ...room, countIn, kind }),
  /** Plays the mix from `fromSec` (to `toSec` or the end) and records the microphone. */
  check: (mix: MixPart[], part: string, fromSec: number, toSec: number | null, room: Room) =>
    invoke<number>("sing_check", { mix, part, fromSec, toSec, ...room }),
  /** The last check's stretch with the recorded voice over the mix (`sing-play` events). */
  checkReplay: (mix: MixPart[], room: Room) => invoke<number>("sing_check_replay", { mix, ...room }),
};

export const LANGS: Record<Lang, string> = { it: "Italiano", en: "English", la: "Latino (ecclesiastico)" };

/** Dictionary a language is sung with: Latin uses the Italian one. */
export const dictLang = (lang: Lang): string => (lang === "la" ? "it" : lang);

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

// --- recording a voicebank ---------------------------------------------------------------------

export type CNote = { pitch: number; beats: number; syl: string | null; word_end: boolean };
export type Phrase = { id: string; group: string; text: string; bpm: number; notes: CNote[]; phonemes: string[]; seconds: number };
export type VoiceProfile = { name: string; voice: string; low: number; high: number };
export type Take = { file: string; seconds: number; peak: number; clipped: boolean; quiet: boolean; sample_rate: number; recorded_at: number };
export type VoiceState = { dir: string; profile: VoiceProfile; phrases: Phrase[]; takes: Record<string, Take> };
export type RecentVoice = { dir: string; name: string };
export type AudioKind = "guide" | "take" | "play";
/** `beats`: cursor on the piano roll from the first note; `t` for a take counts from the first click. */
export type Progress = { job: number; kind: AudioKind; phase: "wait" | "countin" | "sing" | "tail"; t: number; total: number; beats: number; level: number };
export type AudioDone = { job: number; kind: AudioKind; id: string; take: Take | null; error: string | null; cancelled: boolean };
export type DevInfo = { name: string; bluetooth: boolean; default: boolean };
export type Devices = { inputs: DevInfo[]; outputs: DevInfo[] };
export type AudioStatus = { input: DevInfo | null; output: DevInfo; input_switched: boolean; input_rate: number; output_rate: number };

export const voiceApi = {
  recent: () => invoke<RecentVoice[]>("voice_recent"),
  create: (parent: string, profile: VoiceProfile) => invoke<VoiceState>("voice_create", { parent, profile }),
  open: (dir: string) => invoke<VoiceState>("voice_open", { dir }),
  addPiece: (dir: string, path: string, part: string) => invoke<VoiceState>("voice_add_piece", { dir, path, part }),
  removeGroup: (dir: string, group: string) => invoke<VoiceState>("voice_remove_group", { dir, group }),
};

/** Audio engine: each start returns a job id, carried by the `audio-progress` / `audio-done` events. */
export const audioApi = {
  devices: () => invoke<Devices>("audio_devices"),
  /** null = automatic choice (a non-Bluetooth microphone if possible). */
  open: (input: string | null, output: string | null) => invoke<AudioStatus>("audio_open", { input, output }),
  close: () => invoke<void>("audio_close"),
  guide: (dir: string, id: string, fromBeats: number, speed: number, looping: boolean) =>
    invoke<number>("audio_guide", { dir, id, fromBeats, speed, looping }),
  record: (dir: string, id: string, guide: boolean, startNote: boolean) => invoke<number>("audio_record", { dir, id, guide, startNote }),
  playTake: (dir: string, id: string, fromSec: number) => invoke<number>("audio_play_take", { dir, id, fromSec }),
  note: (pitch: number, seconds: number) => invoke<void>("audio_note", { pitch, seconds }),
  stop: () => invoke<void>("audio_stop"),
};

/** Comfortable choral ranges (MIDI), editable when creating the singer. */
export const VOICE_TYPES: Record<string, { label: string; low: number; high: number }> = {
  soprano: { label: "Soprano", low: 60, high: 79 },
  contralto: { label: "Contralto", low: 55, high: 74 },
  tenore: { label: "Tenore", low: 48, high: 67 },
  baritono: { label: "Baritono", low: 45, high: 64 },
  basso: { label: "Basso", low: 40, high: 60 },
};

const NOTE_IT = ["Do", "Do♯", "Re", "Mi♭", "Mi", "Fa", "Fa♯", "Sol", "La♭", "La", "Si♭", "Si"];
/** MIDI 60 = Do4 (C4). */
export const noteName = (m: number) => `${NOTE_IT[((m % 12) + 12) % 12]}${Math.floor(m / 12) - 1}`;
