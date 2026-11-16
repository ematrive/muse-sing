import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  AudioLines,
  CheckCircle2,
  Download,
  FilePlus2,
  FileMusic,
  FolderOpen,
  Headphones,
  Loader2,
  Mic2,
  Music,
  Play,
  RefreshCw,
  Repeat,
  Save,
  Square,
  Upload,
  Volume2,
  VolumeX,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ACCOMPANIMENT,
  api,
  errorText,
  LANGS,
  projectApi,
  singApi,
  type Lang,
  type MixPart,
  type OpenedProject,
  type PartCfg,
  type PitchMode,
  type PlayDone,
  type PlayProgress,
  type Project as ProjectData,
  type PartEdits,
  type RecentProject,
  type Roll,
  type SingDone,
  type SingerInfo,
  type SingersList,
  type SingProgress,
} from "../lib/api";
import { Editor } from "./project/Editor";
import { ScoreRoll } from "./project/ScoreRoll";

function fmtDuration(sec: number) {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

function panLabel(p: number) {
  if (Math.abs(p) < 0.05) return "C";
  return `${p < 0 ? "S" : "D"} ${Math.round(Math.abs(p) * 100)}`;
}

function singerLabel(s: SingerInfo, lang: Lang) {
  const notes = [];
  if (!s.engine) notes.push("non supportato");
  if (!s.langs.includes(lang)) notes.push(`senza dizionario ${lang}`);
  return notes.length ? `${s.name} (${notes.join(", ")})` : s.name;
}

const TEMPOS = [1, 0.9, 0.8, 0.7, 0.6];
const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
const join = (dir: string, name: string) => (dir ? `${dir.replace(/[\\/]$/, "")}/${name}` : name);
const fileSafe = (s: string) => s.replace(/[/\\:*?"<>|]/g, "-").trim() || "Muse Sing";

function readMine(): string | null {
  try {
    return localStorage.getItem("musesing.myPart");
  } catch {
    return null;
  }
}

/** Mix line (volume, pan, mute, solo) shared by the voices and the accompaniment. */
function MixControls({
  volume,
  pan,
  mute,
  solo,
  onChange,
}: {
  volume: number;
  pan: number;
  mute: boolean;
  solo: boolean;
  onChange: (patch: Partial<{ volume_db: number; pan: number; mute: boolean; solo: boolean }>) => void;
}) {
  return (
    <>
      <label className="flex items-center gap-2">
        Volume
        <input
          type="range"
          min={-24}
          max={6}
          step={1}
          value={volume}
          onChange={(e) => onChange({ volume_db: Number(e.target.value) })}
          onDoubleClick={() => onChange({ volume_db: 0 })}
          className="w-28 accent-violet-600"
        />
        <span className="w-12 tabular-nums">
          {volume > 0 ? "+" : ""}
          {volume} dB
        </span>
      </label>
      <label className="flex items-center gap-2">
        Pan
        <input
          type="range"
          min={-1}
          max={1}
          step={0.05}
          value={pan}
          onChange={(e) => onChange({ pan: Number(e.target.value) })}
          onDoubleClick={() => onChange({ pan: 0 })}
          className="w-24 accent-violet-600"
        />
        <span className="w-10 tabular-nums">{panLabel(pan)}</span>
      </label>
      <button
        className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 ${mute ? "bg-zinc-200 text-zinc-800 dark:bg-zinc-700 dark:text-zinc-100" : "hover:text-zinc-800 dark:hover:text-zinc-200"}`}
        onClick={() => onChange({ mute: !mute })}
        title="Escludi dall'ascolto e dall'export (senza rigenerare)"
      >
        {mute ? <VolumeX className="h-3.5 w-3.5" /> : <Volume2 className="h-3.5 w-3.5" />}
        {mute ? "muta" : "attiva"}
      </button>
      <button
        className={`rounded px-1.5 py-0.5 font-semibold ${solo ? "bg-amber-400 text-amber-950" : "hover:text-zinc-800 dark:hover:text-zinc-200"}`}
        onClick={() => onChange({ solo: !solo })}
        title="Ascolta solo le tracce con «solo» attivo"
      >
        solo
      </button>
    </>
  );
}

export default function ProjectView({ active }: { active: boolean }) {
  const [opened, setOpened] = useState<Omit<OpenedProject, "project"> | null>(null);
  const [project, setProjectRaw] = useState<ProjectData | null>(null);
  const [dirty, setDirty] = useState(false);
  const [singers, setSingers] = useState<SingersList | null>(null);
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [roll, setRoll] = useState<Roll | null>(null);
  const [mine, setMine] = useState<string | null>(readMine);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<{ text: string; path?: string } | null>(null);

  // render
  const [renderJob, setRenderJob] = useState<number | null>(null);
  const [progress, setProgress] = useState<SingProgress | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [rendered, setRendered] = useState<{ sig: string; seconds: number; warnings: string[] } | null>(null);
  const [showWarnings, setShowWarnings] = useState(false);
  // playback and export
  const [play, setPlay] = useState<{ job: number; t: number; total: number } | null>(null);
  const [cursor, setCursor] = useState(0);
  const [loop, setLoop] = useState<[number, number] | null>(null);
  const [exporting, setExporting] = useState(false);
  const [guideDb, setGuideDb] = useState(-10);

  const renderJobRef = useRef<number | null>(null);
  renderJobRef.current = renderJob;
  const playJobRef = useRef(0);
  const playingRef = useRef(play);
  playingRef.current = play;
  const seekTimer = useRef<number | undefined>(undefined);
  /** Play from here when the running render is done («Applica e ascolta»). */
  const playAfterRender = useRef<number | null>(null);
  const startPlayRef = useRef<(from: number, range: [number, number] | null) => void>(() => {});
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  const setProject = useCallback((f: (p: ProjectData) => ProjectData) => {
    setProjectRaw((p) => (p ? f(p) : p));
    setDirty(true);
  }, []);

  const loadSingers = useCallback(async () => {
    try {
      setSingers(await api.listSingers());
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  const loadRecent = useCallback(() => {
    projectApi.recent().then(setRecent, () => {});
  }, []);

  /** Unsaved changes: ask before throwing them away. */
  const confirmDiscard = useCallback(async () => {
    if (!dirtyRef.current) return true;
    return ask("Il progetto ha modifiche non salvate. Vuoi continuare e perderle?", {
      title: "Modifiche non salvate",
      kind: "warning",
      okLabel: "Continua senza salvare",
      cancelLabel: "Annulla",
    });
  }, []);

  const show = useCallback((o: OpenedProject, isDirty: boolean) => {
    const { project: p, ...rest } = o;
    setOpened(rest);
    setProjectRaw(p);
    setDirty(isDirty);
    setRendered(null);
    setNotice(null);
    setLoop(null);
    setCursor(0);
  }, []);

  const openPath = useCallback(
    async (path: string) => {
      if (!(await confirmDiscard())) return;
      setError(null);
      setBusy(true);
      try {
        if (play) await singApi.stop();
        if (path.toLowerCase().endsWith(".musesing")) show(await projectApi.open(path), false);
        else if (path.toLowerCase().endsWith(".mscz")) show(await projectApi.create(path), true);
        else throw "Apri un file MuseScore (.mscz) o un progetto Muse Sing (.musesing)";
        loadRecent();
      } catch (e) {
        setError(errorText(e));
      } finally {
        setBusy(false);
      }
    },
    [confirmDiscard, show, loadRecent, play],
  );

  // a singer for every part: one saved in the project but moved between OpenUtau's folder and
  // Muse Sing's is found again by its folder; a part with none gets the one saved for that part
  // name, else the language's
  useEffect(() => {
    if (!project || !singers) return;
    const ids = new Set(singers.singers.map((s) => s.id));
    const bare = (id: string) => id.replace(/^musesing:/, "");
    if (project.parts.every((p) => ids.has(p.singer))) return;
    const fixed = project.parts.map((part) => {
      if (ids.has(part.singer)) return part;
      if (part.singer) {
        const moved = singers.singers.find((s) => bare(s.id) === bare(part.singer));
        return moved ? { ...part, singer: moved.id } : part;
      }
      const saved = singers.parts[part.name.toLowerCase()];
      return saved ? { ...part, singer: saved.singer, color: saved.color } : { ...part, singer: singers.selected[project.lang] ?? "" };
    });
    if (JSON.stringify(fixed) !== JSON.stringify(project.parts)) setProjectRaw({ ...project, parts: fixed });
  }, [project, singers]);

  // the score roll follows the score and the tempo
  useEffect(() => {
    if (!opened || !project) return;
    let stale = false;
    projectApi.roll(opened.score_path, project.tempo_factor).then(
      (r) => !stale && setRoll(r),
      (e) => !stale && setError(errorText(e)),
    );
    return () => {
      stale = true;
    };
  }, [opened, project?.tempo_factor]); // eslint-disable-line react-hooks/exhaustive-deps

  const activeRef = useRef(active);
  activeRef.current = active;
  const openRef = useRef(openPath);
  openRef.current = openPath;

  useEffect(() => {
    loadSingers();
    loadRecent();
    const un = getCurrentWebview().onDragDropEvent((ev) => {
      if (!activeRef.current) return;
      const t = ev.payload.type;
      if (t === "enter" || t === "over") setDragging(true);
      else if (t === "leave") setDragging(false);
      else if (t === "drop") {
        setDragging(false);
        const f = ev.payload.paths.find((p) => /\.(mscz|musesing)$/i.test(p)) ?? ev.payload.paths[0];
        if (f) openRef.current(f);
      }
    });
    const unClose = getCurrentWindow().onCloseRequested(async (e) => {
      if (!dirtyRef.current) return;
      const ok = await ask("Il progetto ha modifiche non salvate. Vuoi uscire lo stesso?", {
        title: "Modifiche non salvate",
        kind: "warning",
        okLabel: "Esci senza salvare",
        cancelLabel: "Annulla",
      });
      if (!ok) e.preventDefault();
    });
    return () => {
      un.then((f) => f());
      unClose.then((f) => f());
    };
  }, [loadSingers, loadRecent]);

  // --- render and playback events ----------------------------------------------------------------

  const sigRef = useRef("");
  useEffect(() => {
    const subs = [
      listen<SingProgress>("sing-progress", (e) => {
        if (e.payload.job === renderJobRef.current) setProgress(e.payload);
      }),
      listen<SingDone>("sing-done", (e) => {
        const d = e.payload;
        if (d.job !== renderJobRef.current) return;
        setRenderJob(null);
        setProgress(null);
        setCancelling(false);
        const then = playAfterRender.current;
        playAfterRender.current = null;
        if (d.error) setError(d.error);
        else if (!d.cancelled) {
          setRendered({ sig: sigRef.current, seconds: d.seconds, warnings: d.warnings });
          setCursor(then ?? 0);
          if (then !== null) window.setTimeout(() => startPlayRef.current(then, null), 50);
        }
      }),
      listen<PlayProgress>("sing-play", (e) => {
        if (e.payload.job === playJobRef.current) setPlay(e.payload);
      }),
      listen<PlayDone>("sing-play-done", (e) => {
        if (e.payload.job !== playJobRef.current) return;
        const p = playingRef.current;
        setCursor(p && p.total - p.t > 0.3 ? p.t : 0);
        setPlay(null);
        if (e.payload.error) setError(e.payload.error);
      }),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);

  // --- derived -----------------------------------------------------------------------------------

  const analysis = opened?.analysis ?? null;
  const parts = project?.parts ?? [];
  const sung = parts.filter((p) => p.sing);
  const singerOf = (p: PartCfg) => singers?.singers.find((s) => s.id === p.singer);
  const hasInstruments = !!analysis?.parts.some((p) => !p.sung);
  const accompOn = !!project?.accompaniment.enabled && hasInstruments;
  const renderParts = sung.map((p) => ({ name: p.name, singer: p.singer, color: p.color }));
  const renderEdits: Record<string, PartEdits> = Object.fromEntries(sung.filter((p) => project?.edits[p.name]).map((p) => [p.name, project!.edits[p.name]]));
  const sig =
    project && opened ? JSON.stringify([opened.score_path, project.lang, project.pitch, project.tempo_factor, accompOn, renderParts, renderEdits]) : "";
  sigRef.current = sig;
  const upToDate = !!rendered && rendered.sig === sig;
  const mix: MixPart[] = useMemo(() => {
    if (!project) return [];
    const m: MixPart[] = project.parts.filter((p) => p.sing).map(({ name, volume_db, pan, mute, solo }) => ({ name, volume_db, pan, mute, solo }));
    const a = project.accompaniment;
    m.push({ name: ACCOMPANIMENT, volume_db: a.volume_db, pan: a.pan, mute: a.mute, solo: a.solo });
    return m;
  }, [project]);
  const mixKey = JSON.stringify(mix);
  const missingSinger = sung.filter((p) => !singerOf(p));
  const notEngine = sung.filter((p) => singerOf(p) && !singerOf(p)!.engine);
  const rendering = renderJob !== null;
  const diffsinger = (singers?.singers ?? []).filter((s) => s.singer_type === "diffsinger");
  const total = rendered?.seconds ?? 0;
  const pos = play ? play.t : cursor;
  const nothing = sung.length === 0 && !accompOn;

  // --- actions -----------------------------------------------------------------------------------

  const newFromScore = async () => {
    const f = await open({ multiple: false, filters: [{ name: "MuseScore", extensions: ["mscz"] }] });
    if (typeof f === "string") openPath(f);
  };

  const openProject = async () => {
    const f = await open({ multiple: false, filters: [{ name: "Progetto Muse Sing", extensions: ["musesing"] }] });
    if (typeof f === "string") openPath(f);
  };

  const saveAs = async (forceDialog: boolean) => {
    if (!project || !opened) return;
    let path = opened.path;
    if (!path || forceDialog) {
      const picked = await save({
        defaultPath: join(opened.default_dir, `${fileSafe(project.title)}.musesing`),
        filters: [{ name: "Progetto Muse Sing", extensions: ["musesing"] }],
      });
      if (!picked) return;
      path = picked;
    }
    setError(null);
    try {
      const written = await projectApi.save(path, project, opened.score_path);
      setOpened({ ...opened, path: written });
      setDirty(false);
      loadRecent();
    } catch (e) {
      setError(errorText(e));
    }
  };

  // Cmd+S / Cmd+Shift+S
  const saveRef = useRef(saveAs);
  saveRef.current = saveAs;
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        saveRef.current(e.shiftKey);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);

  const reloadScore = async () => {
    if (!project) return;
    const f = await open({ multiple: false, filters: [{ name: "MuseScore", extensions: ["mscz"] }] });
    if (typeof f !== "string") return;
    setError(null);
    setBusy(true);
    try {
      const o = await projectApi.reloadScore(project, f);
      const keepPath = opened?.path ?? null;
      show({ ...o, path: keepPath, default_dir: opened?.default_dir ?? o.default_dir }, true);
      setNotice({ text: `Partitura aggiornata da ${baseName(f)}: genera di nuovo l'audio.` });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const setPart = (name: string, patch: Partial<PartCfg>) =>
    setProject((p) => ({ ...p, parts: p.parts.map((x) => (x.name === name ? { ...x, ...patch } : x)) }));

  const startRender = async (thenPlayFrom: number | null = null) => {
    if (!project || !opened) return;
    playAfterRender.current = thenPlayFrom;
    setError(null);
    setNotice(null);
    if (play) await singApi.stop();
    try {
      const job = await singApi.start({
        path: opened.score_path,
        lang: project.lang,
        pitch: project.pitch,
        parts: renderParts,
        tempo_factor: project.tempo_factor,
        accompaniment: accompOn,
        edits: renderEdits,
      });
      renderJobRef.current = job;
      setRenderJob(job);
      setProgress(null);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const cancelRender = async () => {
    setCancelling(true);
    await singApi.cancel();
  };

  const startPlay = useCallback(
    async (from: number, range: [number, number] | null) => {
      setError(null);
      try {
        const job = await singApi.play(JSON.parse(mixKey) as MixPart[], from, range);
        playJobRef.current = job;
        setPlay({ job, t: from, total: rendered?.seconds ?? 0 });
      } catch (e) {
        setError(errorText(e));
      }
    },
    [mixKey, rendered],
  );
  startPlayRef.current = startPlay;

  // «Applica e ascolta»: after the edit is in the project (next render of this view), render the
  // changed phrases if needed, then play
  const [applyFrom, setApplyFrom] = useState<number | null>(null);
  useEffect(() => {
    if (applyFrom === null) return;
    setApplyFrom(null);
    (async () => {
      if (upToDate) return startPlay(applyFrom, null);
      if (play) await singApi.stop();
      startRender(applyFrom);
    })();
  }, [applyFrom]); // eslint-disable-line react-hooks/exhaustive-deps

  const stopPlay = () => singApi.stop();

  // volume / pan / mute / solo / loop changed while playing: start again from the same point
  const loopKey = JSON.stringify(loop);
  const firstMix = useRef(true);
  useEffect(() => {
    if (firstMix.current) {
      firstMix.current = false;
      return;
    }
    if (!playingRef.current) return;
    const t = window.setTimeout(() => {
      const p = playingRef.current;
      if (p) startPlay(p.t, loop);
    }, 250);
    return () => clearTimeout(t);
  }, [mixKey, loopKey, startPlay]); // eslint-disable-line react-hooks/exhaustive-deps

  const seek = (t: number) => {
    if (!upToDate) return setCursor(t);
    if (!play) return setCursor(t);
    setPlay({ ...play, t });
    clearTimeout(seekTimer.current);
    seekTimer.current = window.setTimeout(() => startPlay(t, loop), 150);
  };

  const exportAudio = async () => {
    if (!project || !opened) return;
    setError(null);
    const out = await save({
      defaultPath: join(opened.default_dir, `${fileSafe(project.title)}.mp3`),
      filters: [
        { name: "MP3", extensions: ["mp3"] },
        { name: "FLAC (senza perdita)", extensions: ["flac"] },
        { name: "WAV", extensions: ["wav"] },
      ],
    });
    if (!out) return;
    const path = /\.(mp3|flac|wav)$/i.test(out) ? out : `${out}.mp3`;
    setExporting(true);
    try {
      const written = await singApi.export(mix, path);
      setNotice({ text: `Esportato: ${baseName(written)}`, path: written });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setExporting(false);
    }
  };

  const exportGuides = async () => {
    if (!opened) return;
    setError(null);
    const dir = await open({ directory: true, multiple: false, defaultPath: opened.default_dir, title: "Cartella per le basi di studio" });
    if (typeof dir !== "string") return;
    setExporting(true);
    try {
      const files = await singApi.exportGuides(mix, dir, "mp3", guideDb);
      setNotice({ text: `Basi di studio: ${files.length} file MP3 in ${baseName(dir)}`, path: files[0] });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setExporting(false);
    }
  };

  const chooseMine = (name: string | null) => {
    setMine(name);
    try {
      if (name) localStorage.setItem("musesing.myPart", name);
      else localStorage.removeItem("musesing.myPart");
    } catch {
      /* per-viewer convenience only */
    }
  };

  // --- view --------------------------------------------------------------------------------------

  const renderPct = progress ? ((progress.index + (progress.phrases ? progress.phrase / progress.phrases : 0)) / Math.max(1, progress.parts)) * 100 : 0;
  const title = project ? (opened?.path ? baseName(opened.path) : `${project.title} (non salvato)`) : "";

  return (
    <main className={`relative flex-1 space-y-4 overflow-auto p-5 ${active ? "" : "hidden"}`}>
      {dragging && (
        <div className="pointer-events-none absolute inset-3 z-10 flex items-center justify-center rounded-2xl border-2 border-dashed border-violet-500 bg-violet-500/10 text-lg font-medium text-violet-700 dark:text-violet-300">
          Rilascia qui una partitura (.mscz) o un progetto (.musesing)
        </div>
      )}

      <section className="flex flex-wrap items-center gap-2">
        <button className="btn" onClick={newFromScore} disabled={busy || rendering}>
          <FilePlus2 className="h-4 w-4" /> Nuovo da partitura…
        </button>
        <button className="btn" onClick={openProject} disabled={busy || rendering}>
          <FolderOpen className="h-4 w-4" /> Apri progetto…
        </button>
        {project && (
          <>
            <button className="btn" onClick={() => saveAs(false)} disabled={busy} title="Salva (⌘S)">
              <Save className="h-4 w-4" /> Salva
            </button>
            <button className="btn" onClick={() => saveAs(true)} disabled={busy} title="Salva con nome (⇧⌘S)">
              Salva con nome…
            </button>
            <span className="ml-2 truncate text-zinc-500" title={opened?.path ?? ""}>
              {title}
              {dirty && <span className="ml-1 text-violet-600" title="Modifiche non salvate">●</span>}
            </span>
          </>
        )}
      </section>

      {!project || !analysis ? (
        <>
          <button
            onClick={newFromScore}
            className="flex w-full flex-col items-center justify-center gap-3 rounded-2xl border-2 border-dashed border-zinc-300 py-14 text-zinc-500 hover:border-violet-400 hover:text-violet-600 dark:border-zinc-700"
          >
            {busy ? <Loader2 className="h-8 w-8 animate-spin" /> : <Upload className="h-8 w-8" />}
            <span className="text-base font-medium">Trascina qui una partitura MuseScore (.mscz) o un progetto Muse Sing</span>
            <span>oppure clicca per scegliere la partitura</span>
          </button>
          {recent.length > 0 && (
            <section className="card">
              <div className="label">Progetti recenti</div>
              <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
                {recent.map((r) => (
                  <button key={r.path} className="flex w-full items-center gap-3 py-2 text-left hover:text-violet-600" onClick={() => openPath(r.path)}>
                    <Music className="h-4 w-4 shrink-0 text-violet-600" />
                    <span className="font-medium">{r.title}</span>
                    <span className="truncate text-xs text-zinc-500">{r.path}</span>
                  </button>
                ))}
              </div>
            </section>
          )}
        </>
      ) : (
        <>
          <section className="card flex items-start gap-3">
            <FileMusic className="mt-0.5 h-6 w-6 shrink-0 text-violet-600" />
            <div className="min-w-0 flex-1">
              <div className="text-base font-semibold">{project.title}</div>
              <div className="truncate text-zinc-500">
                {analysis.file_name} · {analysis.time_sig} · {/^(6|9|12)\/8$/.test(analysis.time_sig) ? "♩." : "♩"} = {analysis.bpm} ·{" "}
                {fmtDuration(analysis.duration_sec)}
              </div>
            </div>
            <button className="btn" onClick={reloadScore} disabled={busy || rendering} title="Sostituisci la partitura con una versione aggiornata, tenendo le scelte">
              <RefreshCw className="h-4 w-4" /> Ricarica partitura…
            </button>
          </section>

          {analysis.warnings.length > 0 && (
            <section className="rounded-xl border border-amber-300 bg-amber-50 p-3 text-amber-900 dark:border-amber-700 dark:bg-amber-950/40 dark:text-amber-200">
              {analysis.warnings.map((w) => (
                <div key={w} className="flex gap-2">
                  <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" /> <span>{w}</span>
                </div>
              ))}
            </section>
          )}

          <section className="card grid gap-4 sm:grid-cols-3">
            <div>
              <label className="label">Lingua del testo</label>
              <select className="field" aria-label="Lingua del testo" value={project.lang} onChange={(e) => setProject((p) => ({ ...p, lang: e.target.value as Lang }))} disabled={rendering}>
                {Object.entries(LANGS).map(([k, v]) => (
                  <option key={k} value={k}>
                    {v}
                    {k === analysis.lang ? " (riconosciuta)" : ""}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="label">Intonazione</label>
              <select className="field" aria-label="Intonazione" value={project.pitch} onChange={(e) => setProject((p) => ({ ...p, pitch: e.target.value as PitchMode }))} disabled={rendering}>
                <option value="model">Naturale (decide il cantante)</option>
                <option value="notes">Esatta (ferma sulle note)</option>
              </select>
            </div>
            <div>
              <label className="label">Velocità</label>
              <select
                className="field"
                aria-label="Velocità"
                value={project.tempo_factor}
                onChange={(e) => setProject((p) => ({ ...p, tempo_factor: Number(e.target.value) }))}
                disabled={rendering}
              >
                {TEMPOS.map((t) => (
                  <option key={t} value={t}>
                    {t === 1 ? "100% (come scritto)" : `${Math.round(t * 100)}% (per studiare)`}
                  </option>
                ))}
              </select>
            </div>
          </section>

          <section className="card">
            <div className="label flex items-center justify-between">
              <span>Voci, cantanti e mix</span>
              <button className="text-violet-600 hover:underline" onClick={loadSingers} title="Rileggi i cantanti installati" disabled={rendering}>
                <RefreshCw className="inline h-3 w-3" /> aggiorna cantanti
              </button>
            </div>
            {singers && diffsinger.length === 0 && (
              <div className="mb-2 rounded-md border border-zinc-300 p-2 text-zinc-500 dark:border-zinc-700">
                Nessun cantante trovato in {[...singers.folders, singers.own_folder].filter(Boolean).join(", ")}. Copia un voicebank DiffSinger in una di queste
                cartelle e premi «aggiorna cantanti».
              </div>
            )}
            <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
              {parts.map((p) => {
                const s = singerOf(p);
                const info = analysis.parts.find((x) => x.name === p.name);
                return (
                  <div key={p.name} className="py-2.5">
                    <div className="flex items-start gap-3">
                      <input
                        type="checkbox"
                        className="mt-1 accent-violet-600"
                        disabled={rendering}
                        checked={p.sing}
                        onChange={() => setPart(p.name, { sing: !p.sing })}
                        aria-label={`Canta ${p.name}`}
                      />
                      <div className="min-w-0 flex-1">
                        <div className="font-medium">
                          {p.name}{" "}
                          {info && (
                            <span className="font-normal text-zinc-500">
                              · {info.notes} note · {info.syllables} sillabe
                            </span>
                          )}
                          <button
                            className={`ml-2 rounded px-1.5 text-xs ${mine === p.name ? "bg-violet-600 text-white" : "text-zinc-400 hover:text-violet-600"}`}
                            onClick={() => chooseMine(mine === p.name ? null : p.name)}
                            title="La voce che canti tu: evidenziata nel rullo"
                          >
                            la mia voce
                          </button>
                        </div>
                        {info && <div className="truncate text-xs italic text-zinc-500">{info.preview.replace(/\[[^\]]*\]/g, "")} …</div>}
                      </div>
                      <div className="flex shrink-0 gap-1.5">
                        <select
                          className="field w-52 py-1"
                          value={p.singer}
                          disabled={!p.sing || rendering}
                          onChange={(e) => setPart(p.name, { singer: e.target.value, color: null })}
                          aria-label={`Cantante di ${p.name}`}
                        >
                          <option value="">— cantante —</option>
                          {diffsinger.map((x) => (
                            <option key={x.id} value={x.id} disabled={!x.engine}>
                              {singerLabel(x, project.lang)}
                            </option>
                          ))}
                        </select>
                        {s && s.colors.length > 1 && (
                          <select
                            className="field w-32 py-1"
                            value={p.color ?? ""}
                            disabled={!p.sing || rendering}
                            onChange={(e) => setPart(p.name, { color: e.target.value || null })}
                            title="Colore di voce"
                            aria-label={`Colore di voce di ${p.name}`}
                          >
                            {s.colors.map((col, i) => (
                              <option key={col} value={i === 0 ? "" : col}>
                                {col}
                              </option>
                            ))}
                          </select>
                        )}
                      </div>
                    </div>
                    {p.sing && (
                      <div className="mt-1.5 flex flex-wrap items-center gap-x-5 gap-y-1 pl-7 text-xs text-zinc-500">
                        <MixControls volume={p.volume_db} pan={p.pan} mute={p.mute} solo={p.solo} onChange={(patch) => setPart(p.name, patch)} />
                        {s && !s.engine && <span className="text-amber-600">{s.engine_note}</span>}
                      </div>
                    )}
                  </div>
                );
              })}
              {hasInstruments && (
                <div className="py-2.5">
                  <div className="flex items-start gap-3">
                    <input
                      type="checkbox"
                      className="mt-1 accent-violet-600"
                      disabled={rendering}
                      checked={project.accompaniment.enabled}
                      onChange={() => setProject((p) => ({ ...p, accompaniment: { ...p.accompaniment, enabled: !p.accompaniment.enabled } }))}
                      aria-label="Accompagnamento"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="font-medium">
                        Accompagnamento{" "}
                        <span className="font-normal text-zinc-500">
                          · {analysis.parts.filter((x) => !x.sung).map((x) => x.name).join(", ")} con suono d'organo
                        </span>
                      </div>
                    </div>
                  </div>
                  {project.accompaniment.enabled && (
                    <div className="mt-1.5 flex flex-wrap items-center gap-x-5 gap-y-1 pl-7 text-xs text-zinc-500">
                      <MixControls
                        volume={project.accompaniment.volume_db}
                        pan={project.accompaniment.pan}
                        mute={project.accompaniment.mute}
                        solo={project.accompaniment.solo}
                        onChange={(patch) => setProject((p) => ({ ...p, accompaniment: { ...p.accompaniment, ...patch } }))}
                      />
                    </div>
                  )}
                </div>
              )}
            </div>
          </section>

          <section className="card space-y-3">
            <div className="flex items-center gap-2 font-semibold">
              <AudioLines className="h-4 w-4 text-violet-600" /> Audio cantato
            </div>
            {rendering ? (
              <div className="space-y-2">
                <div className="h-2 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
                  <div className="h-full bg-violet-600 transition-all" style={{ width: `${renderPct}%` }} />
                </div>
                <div className="flex items-center gap-3">
                  <Loader2 className="h-4 w-4 animate-spin text-violet-600" />
                  <span className="flex-1 text-zinc-600 dark:text-zinc-300">
                    {cancelling
                      ? "Annullamento… (finisce la frase in corso)"
                      : progress
                        ? `${progress.part}: ${progress.cached ? "già pronta" : `frase ${Math.min(progress.phrase + 1, progress.phrases)} di ${progress.phrases}`} · voce ${progress.index + 1} di ${progress.parts}`
                        : "Preparo i cantanti…"}
                  </span>
                  <button className="btn" onClick={cancelRender} disabled={cancelling}>
                    <X className="h-4 w-4" /> Annulla
                  </button>
                </div>
              </div>
            ) : (
              <div className="flex flex-wrap items-center gap-3">
                <button className="btn-primary" onClick={() => startRender()} disabled={busy || nothing || missingSinger.length > 0 || notEngine.length > 0}>
                  <Mic2 className="h-4 w-4" /> {rendered ? "Genera di nuovo" : "Genera audio"}
                </button>
                {nothing ? (
                  <span className="text-zinc-500">Scegli almeno una voce o l'accompagnamento.</span>
                ) : missingSinger.length > 0 ? (
                  <span className="text-zinc-500">Scegli il cantante per: {missingSinger.map((p) => p.name).join(", ")}.</span>
                ) : notEngine.length > 0 ? (
                  <span className="text-amber-600">{notEngine.map((p) => p.name).join(", ")}: il cantante scelto non è supportato.</span>
                ) : rendered && !upToDate ? (
                  <span className="text-amber-600">Hai cambiato voci, cantanti, lingua, intonazione, velocità o ritocchi: genera di nuovo.</span>
                ) : !rendered ? (
                  <span className="text-zinc-500">Le voci già generate con le stesse scelte si riusano subito.</span>
                ) : null}
              </div>
            )}

            {rendered && upToDate && !rendering && (
              <>
                <div className="flex flex-wrap items-center gap-3">
                  <button className="btn" onClick={() => (play ? stopPlay() : startPlay(loop && (cursor < loop[0] || cursor > loop[1]) ? loop[0] : cursor, loop))}>
                    {play ? <Square className="h-4 w-4" /> : <Play className="h-4 w-4" />} {play ? "Ferma" : "Ascolta"}
                  </button>
                  <input
                    type="range"
                    min={0}
                    max={Math.max(1, total)}
                    step={0.1}
                    value={Math.min(pos, total)}
                    onChange={(e) => seek(Number(e.target.value))}
                    className="min-w-40 flex-1 accent-violet-600"
                    aria-label="Posizione"
                  />
                  <span className="w-24 text-right tabular-nums text-zinc-500">
                    {fmtDuration(pos)} / {fmtDuration(total)}
                  </span>
                  {loop && (
                    <span className="inline-flex items-center gap-1 rounded bg-amber-100 px-2 py-0.5 text-xs text-amber-900 dark:bg-amber-900/40 dark:text-amber-200">
                      <Repeat className="h-3.5 w-3.5" /> {fmtDuration(loop[0])}–{fmtDuration(loop[1])}
                      <button onClick={() => setLoop(null)} title="Togli la ripetizione">
                        <X className="h-3.5 w-3.5" />
                      </button>
                    </span>
                  )}
                </div>
                <div className="flex flex-wrap items-center gap-3">
                  <button className="btn" onClick={exportAudio} disabled={exporting}>
                    {exporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />} Esporta…
                  </button>
                  <button
                    className="btn"
                    onClick={exportGuides}
                    disabled={exporting || sung.length < 2}
                    title="Un MP3 per ogni voce, con quella voce in evidenza e le altre più basse"
                  >
                    <Headphones className="h-4 w-4" /> Basi di studio…
                  </button>
                  <label className="flex items-center gap-2 text-xs text-zinc-500">
                    altre voci
                    <select className="field w-24 py-0.5" value={guideDb} onChange={(e) => setGuideDb(Number(e.target.value))}>
                      {[-6, -10, -15, -20].map((d) => (
                        <option key={d} value={d}>
                          {d} dB
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                {rendered.warnings.length > 0 && (
                  <div className="text-xs text-amber-700 dark:text-amber-300">
                    <button className="hover:underline" onClick={() => setShowWarnings(!showWarnings)}>
                      <AlertTriangle className="mr-1 inline h-3.5 w-3.5" />
                      {rendered.warnings.length} avvisi di pronuncia {showWarnings ? "▴" : "▾"}
                    </button>
                    {showWarnings && (
                      <ul className="mt-1 max-h-40 list-disc overflow-auto pl-5 select-text">
                        {rendered.warnings.map((w, i) => (
                          <li key={i}>{w}</li>
                        ))}
                      </ul>
                    )}
                  </div>
                )}
              </>
            )}

            {notice && (
              <div className="flex items-center gap-3 rounded-lg border border-emerald-300 bg-emerald-50 p-2.5 text-emerald-900 dark:border-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-200">
                <CheckCircle2 className="h-5 w-5 shrink-0" />
                <div className="min-w-0 flex-1 truncate">{notice.text}</div>
                {notice.path && (
                  <button className="btn" onClick={() => revealItemInDir(notice.path!)}>
                    <FolderOpen className="h-4 w-4" /> Mostra nel Finder
                  </button>
                )}
              </div>
            )}
          </section>

          {roll && roll.parts.length > 0 && (
            <section className="card">
              <ScoreRoll
                parts={roll.parts}
                cursor={pos}
                playing={!!play}
                highlight={mine}
                loop={loop}
                onSeek={seek}
                onLoop={(r) => {
                  setLoop(r);
                  if (r) setCursor(r[0]);
                }}
              />
            </section>
          )}

          {roll && rendered && sung.length > 0 && (
            <section className="card">
              <Editor
                parts={sung.map((p) => p.name)}
                roll={roll}
                edits={project.edits}
                initialPart={mine}
                rendered={rendered}
                upToDate={upToDate}
                rendering={rendering}
                cursor={pos}
                playing={!!play}
                onChange={(part, e) => setProject((p) => ({ ...p, edits: { ...p.edits, [part]: e } }))}
                onSeek={seek}
                onApply={setApplyFrom}
              />
            </section>
          )}
        </>
      )}

      {error && (
        <section className="flex items-start gap-2 rounded-xl border border-red-300 bg-red-50 p-3 text-red-800 dark:border-red-800 dark:bg-red-950/40 dark:text-red-200">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="flex-1 select-text">{error}</div>
          <button onClick={() => setError(null)}>
            <X className="h-4 w-4" />
          </button>
        </section>
      )}

      <footer className="pt-2 text-xs text-zinc-500">
        L'audio è generato sul Mac con il motore DiffSinger di Muse Sing (derivato da OpenUtau). TIGER DS (tigermeat) è gratuito solo per uso non
        commerciale, citando l'autore.
      </footer>
    </main>
  );
}
