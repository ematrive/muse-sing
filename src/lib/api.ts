import { invoke } from "@tauri-apps/api/core";

export type Lang = "it" | "en";

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

export type SingerInfo = { id: string; name: string; singer_type: string; langs: string[] };
export type SingersList = { singers: SingerInfo[]; folders: string[]; selected: Partial<Record<Lang, string>> };
export type GenerateResult = { output: string; tracks: number; notes: number };

export const api = {
  analyze: (path: string) => invoke<Analysis>("analyze", { path }),
  listSingers: () => invoke<SingersList>("list_singers"),
  generate: (req: { path: string; output: string; lang: Lang; singer: string; parts: string[] }) =>
    invoke<GenerateResult>("generate", { req }),
};

export const LANGS: Record<Lang, string> = { it: "Italiano", en: "English" };

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
