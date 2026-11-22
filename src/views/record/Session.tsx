import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  Circle,
  FolderOpen,
  Mic,
  Pause,
  Play,
  Plus,
  Repeat,
  SkipBack,
  Square,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  audioApi,
  errorText,
  noteName,
  voiceApi,
  VOICE_TYPES,
  type Analysis,
  type AudioDone,
  type AudioKind,
  type Devices,
  type Phrase,
  type Progress,
  type VoiceState,
} from "../../lib/api";
import { MicCheck, type MicState } from "./MicCheck";
import { noteAt, noteStarts, PianoRoll } from "./PianoRoll";
import { serial } from "../../lib/serial";
import { useStored } from "./useStored";

const COUNT_IN_BEATS = 4;
/** Seconds before the first note in a saved take (as in audio.rs). */
const LEAD_IN = 0.6;
const TAIL = 1.0;
const SPEEDS = [0.5, 0.75, 1];
/** Pause before moving on by itself after a good take. */
const ADVANCE_MS = 1800;


export function ErrorBox({ error, onClose }: { error: string | null; onClose: () => void }) {
  if (!error) return null;
  return (
    <section className="flex items-start gap-2 rounded-xl border border-red-300 bg-red-50 p-3 text-red-800 dark:border-red-800 dark:bg-red-950/40 dark:text-red-200">
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
      <div className="flex-1 select-text">{error}</div>
      <button onClick={onClose}>
        <X className="h-4 w-4" />
      </button>
    </section>
  );
}

function fmtMin(sec: number) {
  return `${Math.floor(sec / 60)}:${String(Math.round(sec % 60)).padStart(2, "0")}`;
}

function fmtSec(sec: number) {
  const s = Math.max(0, sec);
  return `${Math.floor(s / 60)}:${(s % 60).toFixed(1).padStart(4, "0")}`;
}

function guessPart(a: Analysis, voice: string) {
  const sung = a.parts.filter((p) => p.sung).map((p) => p.name);
  const keys: Record<string, RegExp> = {
    soprano: /sopr/i,
    contralto: /contr|alto/i,
    tenore: /ten/i,
    baritono: /bar/i,
    basso: /bass/i,
  };
  return sung.find((n) => keys[voice]?.test(n)) ?? sung[0] ?? "";
}

function Toggle({ checked, onChange, children, title }: { checked: boolean; onChange: (v: boolean) => void; children: React.ReactNode; title?: string }) {
  return (
    <label className="flex cursor-pointer items-center gap-1.5 text-zinc-600 dark:text-zinc-400" title={title}>
      <input type="checkbox" className="accent-violet-600" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      {children}
    </label>
  );
}

export function Session({
  voice,
  setVoice,
  active,
  error,
  setError,
}: {
  voice: VoiceState;
  setVoice: (v: VoiceState | null) => void;
  active: boolean;
  error: string | null;
  setError: (e: string | null) => void;
}) {
  const firstTodo = useMemo(() => Math.max(0, voice.phrases.findIndex((p) => !voice.takes[p.id])), []); // eslint-disable-line react-hooks/exhaustive-deps
  const [idx, setIdx] = useState(firstTodo);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [running, setRunning] = useState<AudioKind | null>(null);
  const [cursor, setCursor] = useState(0);
  const [flash, setFlash] = useState(-1);
  const [maxLevel, setMaxLevel] = useState(0);
  const [advance, setAdvance] = useState(false);
  const [piece, setPiece] = useState<{ analysis: Analysis; part: string } | null>(null);
  const [tookOne, setTookOne] = useState(false);

  const [guideOn, setGuideOn] = useStored("rec.guide", true);
  const [startNote, setStartNote] = useStored("rec.startNote", true);
  const [autoNext, setAutoNext] = useStored("rec.autoNext", false);
  const [speed, setSpeed] = useStored("rec.speed", 1);
  const [looping, setLooping] = useStored("rec.loop", false);
  const [inputPref, setInputPref] = useStored("rec.input", "");
  const [outputPref, setOutputPref] = useStored("rec.output", "");

  const [mic, setMic] = useState<MicState>({ state: "opening" });
  const [devices, setDevices] = useState<Devices | null>(null);
  const [issue, setIssue] = useState<string | null>(null);

  const listRef = useRef<HTMLDivElement>(null);
  /** Job whose events we follow; older ones are late events of a replaced job. */
  const jobRef = useRef(0);
  const startingRef = useRef(false);
  /** Playback interrupted by dragging the cursor, to resume on release. */
  const resumeRef = useRef<AudioKind | null>(null);

  const phrase = voice.phrases[Math.min(idx, voice.phrases.length - 1)] as Phrase | undefined;
  const take = phrase ? voice.takes[phrase.id] : undefined;
  const beatSec = phrase ? 60 / phrase.bpm : 0.5;
  const layout = useMemo(() => (phrase ? noteStarts(phrase) : { starts: [], total: 0 }), [phrase]);
  const total = layout.total;

  const latest = useRef({ voice, phrase, autoNext });
  latest.current = { voice, phrase, autoNext };

  // --- devices ---------------------------------------------------------------------------------

  const openAudio = useCallback(() => {
    setMic({ state: "opening" });
    setIssue(null);
    serial(() => audioApi.open(inputPref || null, outputPref || null))
      .then((status) => setMic({ state: "ok", status }))
      .catch((e) => setMic({ state: "error", error: errorText(e) }));
    audioApi
      .devices()
      .then(setDevices)
      .catch(() => {});
  }, [inputPref, outputPref]);

  useEffect(() => {
    if (active) openAudio();
  }, [active, openAudio]);

  useEffect(() => {
    if (!active) return;
    // leaving the tab or the session: microphone off, Bluetooth headphones back to full quality
    return () => {
      serial(() => audioApi.close()).catch(() => {});
    };
  }, [active]);

  // --- events ----------------------------------------------------------------------------------

  useEffect(() => {
    const subs = [
      listen<Progress>("audio-progress", (e) => {
        const p = e.payload;
        if (p.job < jobRef.current) return;
        jobRef.current = p.job;
        setProgress(p);
        setRunning(p.kind);
        if (p.kind === "take" && p.phase === "sing") setMaxLevel((m) => Math.max(m, p.level));
      }),
      listen<AudioDone>("audio-done", (e) => {
        const d = e.payload;
        if (d.job < jobRef.current || (startingRef.current && d.job <= jobRef.current)) return;
        jobRef.current = d.job;
        setRunning(null);
        setProgress(null);
        if (d.error) setError(d.error);
        // a guide or a playback that reached the end starts again from the top next time
        if (d.kind !== "take" && !d.cancelled && !d.error) setCursor(0);
        if (d.take) {
          const { voice: v, phrase: ph, autoNext: auto } = latest.current;
          setVoice({ ...v, takes: { ...v.takes, [d.id]: d.take } });
          setTookOne(true);
          if (auto && !d.cancelled && d.id === ph?.id && !d.take.clipped && !d.take.quiet) setAdvance(true);
        }
      }),
      listen<string>("audio-device", (e) => setIssue(e.payload)),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, [setError, setVoice]);

  // --- actions ---------------------------------------------------------------------------------

  const launch = useCallback(
    async (kind: AudioKind, f: () => Promise<number>) => {
      setError(null);
      setAdvance(false);
      startingRef.current = true;
      setRunning(kind);
      setProgress(null);
      setMaxLevel(0);
      try {
        const job = await f();
        if (job > jobRef.current) jobRef.current = job;
      } catch (e) {
        setRunning(null);
        setError(errorText(e));
      } finally {
        startingRef.current = false;
      }
    },
    [setError],
  );

  const playGuide = useCallback(
    (from: number) => {
      if (!phrase) return;
      launch("guide", () => audioApi.guide(voice.dir, phrase.id, from >= total - 1e-6 ? 0 : from, speed, looping));
    },
    [launch, phrase, voice.dir, total, speed, looping],
  );

  const playTake = useCallback(
    (from: number) => {
      if (!phrase) return;
      const fromSec = from <= 0 || from >= total - 1e-6 ? 0 : LEAD_IN + from * beatSec;
      launch("play", () => audioApi.playTake(voice.dir, phrase.id, fromSec));
    },
    [launch, phrase, voice.dir, total, beatSec],
  );

  const restart = useCallback((kind: AudioKind, from: number) => (kind === "guide" ? playGuide(from) : playTake(from)), [playGuide, playTake]);

  /** Stops the guide or the playback where it is: the next play goes on from there. */
  const pause = useCallback(() => {
    if (progress && (progress.kind === "guide" || progress.kind === "play")) setCursor(Math.max(0, Math.min(total, progress.beats)));
    audioApi.stop();
  }, [progress, total]);

  const toggleGuide = useCallback(() => (running === "guide" ? pause() : running === "take" ? undefined : playGuide(cursor)), [running, pause, playGuide, cursor]);
  const toggleTake = useCallback(() => {
    if (!take || running === "take") return;
    if (running === "play") pause();
    else playTake(cursor);
  }, [take, running, pause, playTake, cursor]);

  const record = useCallback(() => {
    if (!phrase) return;
    if (running === "take") {
      audioApi.stop();
      return;
    }
    launch("take", () => audioApi.record(voice.dir, phrase.id, guideOn, startNote));
  }, [phrase, running, launch, voice.dir, guideOn, startNote]);

  const seek = useCallback(
    (b: number, commit: boolean) => {
      if (running === "take") return;
      if (!commit && (running === "guide" || running === "play")) {
        resumeRef.current = running;
        audioApi.stop();
      }
      setCursor(b);
      if (!commit) return;
      // resume what the drag interrupted, or jump there if something is playing (⏮, Home, , .)
      const kind = resumeRef.current ?? (running === "guide" || running === "play" ? running : null);
      resumeRef.current = null;
      if (kind) restart(kind, b);
    },
    [running, restart],
  );

  const clickNote = useCallback(
    (i: number) => {
      if (!phrase || running === "take") return;
      const b = layout.starts[i];
      setCursor(b);
      if (running === "guide" || running === "play") {
        restart(running, b);
        return;
      }
      const secs = phrase.notes[i].beats * beatSec;
      setFlash(i);
      window.setTimeout(() => setFlash((f) => (f === i ? -1 : f)), Math.min(3, Math.max(0.15, secs)) * 1000 + 100);
      audioApi.note(phrase.notes[i].pitch, secs).catch((e) => setError(errorText(e)));
    },
    [phrase, running, layout, beatSec, restart, setError],
  );

  const stepNote = useCallback(
    (d: number) => {
      const starts = layout.starts;
      if (!starts.length) return;
      let here = -1;
      if (d > 0) here = starts.findIndex((s) => s > cursor + 1e-6);
      else for (let k = starts.length - 1; k >= 0 && here < 0; k--) if (starts[k] < cursor - 1e-6) here = k;
      const i = here < 0 ? (d > 0 ? starts.length - 1 : 0) : here;
      seek(starts[i], true);
    },
    [layout, cursor, seek],
  );

  const go = useCallback(
    (d: number) => {
      if (running) audioApi.stop();
      setAdvance(false);
      setIdx((i) => Math.max(0, Math.min(voice.phrases.length - 1, i + d)));
    },
    [running, voice.phrases.length],
  );

  const nextTodo = useCallback(() => {
    setAdvance(false);
    const n = voice.phrases.length;
    for (let k = 1; k <= n; k++) {
      const j = (idx + k) % n;
      if (!voice.takes[voice.phrases[j].id]) {
        if (running) audioApi.stop();
        setIdx(j);
        return;
      }
    }
  }, [idx, running, voice]);

  // new phrase: cursor back to the start
  useEffect(() => {
    setCursor(0);
    setFlash(-1);
    listRef.current?.querySelector(`[data-id="${CSS.escape(phrase?.id ?? "")}"]`)?.scrollIntoView({ block: "nearest" });
  }, [phrase?.id]);

  // after a good take, move on by itself (if asked)
  useEffect(() => {
    if (!advance) return;
    const t = window.setTimeout(nextTodo, ADVANCE_MS);
    return () => window.clearTimeout(t);
  }, [advance, nextTodo]);

  // keyboard
  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e: KeyboardEvent) => {
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA" || e.metaKey || e.ctrlKey || e.altKey) return;
    // holding a key down must not start and cancel in turn
    if (e.repeat) {
      if (e.code === "Space") e.preventDefault();
      return;
    }
    const k = e.key.toLowerCase();
    if (e.code === "Space") {
      e.preventDefault();
      record();
    } else if (k === "g") toggleGuide();
    else if (k === "p") toggleTake();
    else if (e.key === "Home") seek(0, true);
    else if (e.key === ",") stepNote(-1);
    else if (e.key === ".") stepNote(1);
    else if (e.key === "ArrowRight") go(1);
    else if (e.key === "ArrowLeft") go(-1);
    else if (e.key === "Enter") nextTodo();
    else if (e.key === "Escape") {
      if (advance) setAdvance(false);
      else if (running === "guide" || running === "play") pause();
      else audioApi.stop();
    } else return;
    e.preventDefault();
  };
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => keys.current(e);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);

  // --- pieces ----------------------------------------------------------------------------------

  const addPiece = async () => {
    setError(null);
    const f = await open({ multiple: false, filters: [{ name: "MuseScore", extensions: ["mscz"] }] });
    if (typeof f !== "string") return;
    try {
      const a = await api.analyze(f);
      if (!a.parts.some((p) => p.sung)) throw new Error("questo brano non ha voci con testo");
      setPiece({ analysis: a, part: guessPart(a, voice.profile.voice) });
    } catch (e) {
      setError(errorText(e));
    }
  };

  const confirmPiece = async () => {
    if (!piece) return;
    try {
      const v = await voiceApi.addPiece(voice.dir, piece.analysis.path, piece.part);
      setVoice(v);
      setPiece(null);
      const first = v.phrases.findIndex((p) => p.group === `Brano · ${piece.analysis.title}` && !v.takes[p.id]);
      if (first >= 0) setIdx(first);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const removeGroup = async (group: string) => {
    if (!confirm(`Togliere «${group.replace("Brano · ", "")}» dall'elenco? Le registrazioni già fatte restano nella cartella.`)) return;
    try {
      setVoice(await voiceApi.removeGroup(voice.dir, group));
      setIdx(0);
    } catch (e) {
      setError(errorText(e));
    }
  };

  // --- derived ---------------------------------------------------------------------------------

  const done = voice.phrases.filter((p) => voice.takes[p.id]).length;
  const doneSec = voice.phrases.filter((p) => voice.takes[p.id]).reduce((s, p) => s + p.seconds, 0);
  const totalSec = voice.phrases.reduce((s, p) => s + p.seconds, 0);
  const groups = useMemo(() => {
    const g: { name: string; items: { p: Phrase; i: number }[] }[] = [];
    voice.phrases.forEach((p, i) => {
      if (!g.length || g[g.length - 1].name !== p.group) g.push({ name: p.group, items: [] });
      g[g.length - 1].items.push({ p, i });
    });
    return g;
  }, [voice.phrases]);

  const takePhase = running === "take" && progress?.kind === "take" ? progress.phase : running === "take" ? "wait" : null;
  // where the red line is: guide/playback position, or the take once the phrase starts
  const playing =
    progress && running && progress.kind === running && (running !== "take" || takePhase === "sing" || takePhase === "tail") ? progress.beats : null;
  const highlight = playing !== null && phrase ? noteAt(phrase, playing) : running ? -1 : flash;
  const beatIdx = takePhase === "countin" && progress ? Math.min(COUNT_IN_BEATS - 1, Math.floor(progress.t / beatSec)) : -1;
  const posBeats = playing ?? cursor;
  const firstNote = phrase?.notes[0];

  return (
    <div className="flex min-h-0 flex-1">
      {/* phrase list */}
      <aside className="flex w-72 shrink-0 flex-col border-r border-zinc-200 dark:border-zinc-800">
        <div className="space-y-2 border-b border-zinc-200 p-3 dark:border-zinc-800">
          <div className="flex items-center gap-2">
            <button className="text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200" onClick={() => setVoice(null)} title="Cambia cantante">
              <ArrowLeft className="h-4 w-4" />
            </button>
            <div className="min-w-0 flex-1">
              <div className="truncate font-semibold">{voice.profile.name}</div>
              <div className="text-xs text-zinc-500">
                {VOICE_TYPES[voice.profile.voice]?.label ?? voice.profile.voice} · {noteName(voice.profile.low)}–{noteName(voice.profile.high)}
              </div>
            </div>
            <button className="text-zinc-500 hover:text-violet-600" title="Mostra la cartella nel Finder" onClick={() => revealItemInDir(voice.dir + "/profile.json")}>
              <FolderOpen className="h-4 w-4" />
            </button>
          </div>
          <div>
            <div className="mb-1 flex justify-between text-xs text-zinc-500">
              <span>
                {done} / {voice.phrases.length} frasi
              </span>
              <span>
                {fmtMin(doneSec)} di {fmtMin(totalSec)} min
              </span>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
              <div className="h-full bg-violet-600" style={{ width: `${(done / Math.max(1, voice.phrases.length)) * 100}%` }} />
            </div>
          </div>
          <button className="btn w-full justify-center" onClick={addPiece}>
            <Plus className="h-4 w-4" /> Aggiungi un brano del coro
          </button>
        </div>
        {/* no vertical padding here: sticky headers must stick to the very top edge */}
        <div ref={listRef} className="flex-1 overflow-auto">
          {groups.map((g) => {
            const gDone = g.items.filter(({ p }) => voice.takes[p.id]).length;
            return (
              <div key={g.name} className="pb-1">
                <div className="sticky top-0 z-[1] flex items-center gap-1 border-b border-zinc-200 bg-zinc-50 px-3 py-1.5 text-xs font-semibold text-zinc-500 dark:border-zinc-800 dark:bg-zinc-950">
                  <span className="min-w-0 flex-1 truncate">{g.name}</span>
                  <span className="tabular-nums">
                    {gDone}/{g.items.length}
                  </span>
                  {g.name.startsWith("Brano") && (
                    <button className="ml-1 hover:text-red-600" title="Togli il brano" onClick={() => removeGroup(g.name)}>
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  )}
                </div>
                {g.items.map(({ p, i }) => {
                  const t = voice.takes[p.id];
                  return (
                    <button
                      key={p.id}
                      data-id={p.id}
                      onClick={() => {
                        if (running) audioApi.stop();
                        setAdvance(false);
                        setIdx(i);
                      }}
                      className={`flex w-full items-center gap-2 px-3 py-1 text-left ${
                        i === idx ? "bg-violet-100 dark:bg-violet-950/60" : "hover:bg-zinc-100 dark:hover:bg-zinc-900"
                      }`}
                    >
                      {t ? (
                        t.clipped || t.quiet ? (
                          <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-amber-500" />
                        ) : (
                          <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-emerald-600" />
                        )
                      ) : (
                        <Circle className="h-3.5 w-3.5 shrink-0 text-zinc-300 dark:text-zinc-700" />
                      )}
                      <span className="truncate">{p.text || "(vocalizzo)"}</span>
                    </button>
                  );
                })}
              </div>
            );
          })}
        </div>
      </aside>

      {/* recorder */}
      <section className="flex min-w-0 flex-1 flex-col gap-4 overflow-auto p-5">
        <MicCheck
          mic={mic}
          devices={devices}
          input={inputPref}
          output={outputPref}
          setInput={setInputPref}
          setOutput={setOutputPref}
          issue={issue}
          retry={openAudio}
          disabled={running === "take"}
          showHint={!tookOne}
        />

        {piece && (
          <div className="card flex flex-wrap items-end gap-3">
            <div className="min-w-0 flex-1">
              <div className="label">Brano: {piece.analysis.title}</div>
              <select className="field" value={piece.part} onChange={(e) => setPiece({ ...piece, part: e.target.value })}>
                {piece.analysis.parts
                  .filter((p) => p.sung)
                  .map((p) => (
                    <option key={p.name} value={p.name}>
                      {p.name} · {p.syllables} sillabe
                    </option>
                  ))}
              </select>
            </div>
            <button className="btn-primary" onClick={confirmPiece}>
              <Plus className="h-4 w-4" /> Aggiungi questa voce
            </button>
            <button className="btn" onClick={() => setPiece(null)}>
              Annulla
            </button>
          </div>
        )}

        <ErrorBox error={error} onClose={() => setError(null)} />

        {phrase && (
          <>
            <div className="flex items-center gap-2 text-xs text-zinc-500">
              <span>{phrase.group}</span>·
              <span>
                frase {idx + 1} di {voice.phrases.length}
              </span>
              ·<span>♩ = {phrase.bpm}</span>·<span>{phrase.seconds} s</span>
              <span className="ml-auto font-mono">{phrase.id}</span>
            </div>

            {/* 1. listen */}
            <div className="card relative space-y-2">
              <div className="text-2xl font-semibold tracking-wide">{phrase.text || "Vocalizzo: una sola vocale su tutte le note"}</div>
              <PianoRoll
                phrase={phrase}
                cursor={cursor}
                playing={playing}
                highlight={highlight}
                interactive={running !== "take"}
                onSeek={seek}
                onNote={clickNote}
              />
              <div className="truncate text-xs text-zinc-500">{phrase.notes.map((n) => noteName(n.pitch)).join(" · ")}</div>

              <div className="flex flex-wrap items-center gap-2 border-t border-zinc-200 pt-3 dark:border-zinc-800">
                <button className="btn px-2" onClick={() => seek(0, true)} disabled={running === "take"} title="Torna all'inizio (Home)">
                  <SkipBack className="h-4 w-4" />
                </button>
                <button
                  className={`btn min-w-28 justify-center ${running === "guide" ? "border-violet-400 text-violet-700 dark:text-violet-300" : ""}`}
                  onClick={toggleGuide}
                  disabled={running === "take"}
                  title="Ascolta la melodia dal cursore (G) — pausa per fermarla lì"
                >
                  {running === "guide" ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
                  {running === "guide" ? "Pausa" : cursor > 0 ? "Guida da qui" : "Guida"}
                </button>
                <span className="w-24 text-xs tabular-nums text-zinc-500">
                  {fmtSec(posBeats * beatSec)} / {fmtSec(total * beatSec)}
                </span>
                <div className="flex overflow-hidden rounded-md border border-zinc-300 text-xs dark:border-zinc-700" title="Velocità della guida (non della registrazione)">
                  {SPEEDS.map((s) => (
                    <button
                      key={s}
                      className={`px-2 py-1.5 ${speed === s ? "bg-violet-600 text-white" : "bg-white hover:bg-zinc-100 dark:bg-zinc-900 dark:hover:bg-zinc-800"}`}
                      onClick={() => setSpeed(s)}
                    >
                      {Math.round(s * 100)}%
                    </button>
                  ))}
                </div>
                <button
                  className={`btn px-2 ${looping ? "border-violet-400 bg-violet-50 text-violet-700 dark:bg-violet-950/50 dark:text-violet-300" : ""}`}
                  onClick={() => setLooping(!looping)}
                  title="Ripeti la guida finché non la fermi"
                >
                  <Repeat className="h-4 w-4" /> Loop
                </button>
                <span className="ml-auto text-xs text-zinc-500">Clic su una nota per sentirla · clic o trascina per spostare il cursore</span>
              </div>

              {takePhase && takePhase !== "sing" && takePhase !== "tail" && (
                <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 rounded-xl bg-white/75 dark:bg-zinc-900/75">
                  {takePhase === "wait" ? (
                    <div className="text-2xl font-semibold text-violet-600">Preparati…</div>
                  ) : (
                    <>
                      <div className="text-8xl font-bold tabular-nums text-violet-600">{COUNT_IN_BEATS - beatIdx}</div>
                      <div className="flex gap-2">
                        {Array.from({ length: COUNT_IN_BEATS }, (_, b) => (
                          <span key={b} className={`h-3 w-3 rounded-full ${b <= beatIdx ? "bg-violet-600" : "bg-zinc-300 dark:bg-zinc-700"}`} />
                        ))}
                      </div>
                    </>
                  )}
                  {firstNote && (
                    <div className="text-sm text-zinc-600 dark:text-zinc-300">
                      Prima nota: <b>{noteName(firstNote.pitch)}</b>
                      {firstNote.syl ? ` su «${firstNote.syl}»` : ""}
                    </div>
                  )}
                </div>
              )}
            </div>

            {/* 2. record, 3. check */}
            <div className="card space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <button
                  className={`btn-primary min-w-32 justify-center ${
                    running === "take" ? "bg-zinc-700 hover:bg-zinc-600" : "bg-red-600 hover:bg-red-500"
                  }`}
                  onClick={record}
                  title={`Registra (barra spaziatrice): ${COUNT_IN_BEATS} colpi di conteggio, poi canta`}
                >
                  {running === "take" ? <Square className="h-4 w-4" /> : <Mic className="h-4 w-4" />}
                  {running === "take" ? "Annulla" : take ? "Rifai" : "Registra"}
                </button>
                <button className="btn" disabled={!take || running === "take"} onClick={toggleTake} title="Riascolta la registrazione dal cursore (P)">
                  {running === "play" ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />} {running === "play" ? "Pausa" : "Riascolta"}
                </button>
                <div className="ml-auto flex gap-2">
                  <button className="btn" onClick={() => go(-1)} disabled={idx === 0} title="Frase precedente (←)">
                    <ArrowLeft className="h-4 w-4" />
                  </button>
                  <button className="btn" onClick={() => go(1)} disabled={idx >= voice.phrases.length - 1} title="Frase successiva (→)">
                    <ArrowRight className="h-4 w-4" />
                  </button>
                  <button className="btn-primary" onClick={nextTodo} disabled={done === voice.phrases.length} title="Prossima da registrare (Invio)">
                    Prossima da registrare <ArrowRight className="h-4 w-4" />
                  </button>
                </div>
              </div>

              <div className="flex flex-wrap gap-x-4 gap-y-1">
                <Toggle checked={guideOn} onChange={setGuideOn} title="La melodia suona piano in cuffia mentre canti">
                  melodia in cuffia mentre canto
                </Toggle>
                <Toggle checked={startNote} onChange={setStartNote} title="Negli ultimi due colpi del conteggio suona la prima nota, per partire intonati">
                  nota di partenza nel conteggio
                </Toggle>
                <Toggle checked={autoNext} onChange={setAutoNext} title="Dopo una registrazione senza problemi passa da sola alla prossima frase da registrare">
                  avanti da sola dopo una registrazione buona
                </Toggle>
              </div>

              {running === "take" && (
                <div className="text-sm">
                  {takePhase === "countin" || takePhase === "wait" ? (
                    <span className="text-zinc-500">Conteggio… preparati</span>
                  ) : takePhase === "sing" ? (
                    <span className="font-semibold text-red-600">● Canta!</span>
                  ) : (
                    <span className="text-zinc-500">Silenzio ancora un attimo…</span>
                  )}
                  {maxLevel >= 0.99 && <span className="ml-2 font-semibold text-red-600">Satura: abbassa il guadagno del microfono</span>}
                </div>
              )}
              {advance && (
                <div className="flex items-center gap-2 rounded-md bg-emerald-50 p-2 text-sm text-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-200">
                  <CheckCircle2 className="h-4 w-4" /> Registrata bene: passo alla prossima frase…
                  <button className="btn ml-auto" onClick={() => setAdvance(false)}>
                    Resta qui (Esc)
                  </button>
                </div>
              )}
              {take && !running && !advance && (
                <div
                  className={`flex items-center gap-2 text-xs ${take.clipped || take.quiet ? "text-amber-700 dark:text-amber-400" : "text-emerald-700 dark:text-emerald-400"}`}
                >
                  {take.clipped || take.quiet ? <AlertTriangle className="h-4 w-4" /> : <CheckCircle2 className="h-4 w-4" />}
                  Registrata: {take.seconds} s, picco {(20 * Math.log10(Math.max(take.peak, 1e-6))).toFixed(1)} dB
                  {take.clipped && " — il segnale satura: abbassa il guadagno e rifalla"}
                  {take.quiet && " — troppo bassa: alza il guadagno o avvicinati e rifalla"}
                </div>
              )}
            </div>

            <p className="text-xs text-zinc-500">
              Scorciatoie: <b>spazio</b> registra/annulla · <b>G</b> guida/pausa · <b>P</b> riascolta/pausa · <b>Home</b> cursore all'inizio ·{" "}
              <b>, .</b> nota precedente/successiva · <b>← →</b> frasi · <b>Invio</b> prossima da registrare · <b>Esc</b> ferma. Ogni registrazione
              parte dall'inizio con {COUNT_IN_BEATS} colpi e finisce da sola {TAIL} s dopo l'ultima nota. Per una prova bastano le frasi del registro
              medio e i vocalizzi (circa 15 minuti).
            </p>
          </>
        )}
      </section>
    </div>
  );
}
