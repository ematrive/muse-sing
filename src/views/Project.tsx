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
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ACCOMPANIMENT,
  INSTRUMENTS,
  instrumentFor,
  MAX_TRANSPOSE,
  METRONOME,
  SOUNDFONT_EVENT,
  api,
  isProjectPath,
  isScorePath,
  PROJECT_EXTENSION,
  PROJECT_EXTENSIONS,
  SCORE_EXTENSIONS,
  TIGER_EVENT,
  errorText,
  LANGS,
  AMBIENCES,
  projectApi,
  singApi,
  type Ambience,
  type GuideKind,
  type Instrument,
  type Room,
  type Lang,
  dictLang,
  type MixPart,
  type OpenedProject,
  type PartCfg,
  type PitchMode,
  type PlayDone,
  type CheckDone,
  type CheckProgress,
  type PlayProgress,
  type Project as ProjectData,
  type ProjectSizes,
  type AudioSave,
  type RenderRequest,
  type PartEdits,
  type RecentProject,
  type Roll,
  type SingDone,
  type SingerInfo,
  type SingersList,
  type SingProgress,
  type TigerProgress,
} from "../lib/api";
import { Editor } from "./project/Editor";
import { Info, type AudioState, type InfoPart } from "./project/Info";
import { CheckPanel, OWN_VOICE_DB, summarize, type OwnVoice } from "./project/Check";
import { Mixer } from "./project/Mixer";
import { ScoreRoll } from "./project/ScoreRoll";

function fmtDuration(sec: number) {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

function singerLabel(s: SingerInfo, lang: Lang) {
  const notes = [];
  if (!s.engine) notes.push("non supportato");
  if (!s.langs.includes(dictLang(lang))) notes.push(`senza dizionario ${dictLang(lang)}`);
  return notes.length ? `${s.name} (${notes.join(", ")})` : s.name;
}

const TEMPOS = [1, 0.9, 0.8, 0.7, 0.6];
const MB = 1024 * 1024;
function tigerText(t: TigerProgress) {
  const pct = t.total ? ` ${Math.floor((t.done / t.total) * 100)}%` : "";
  if (t.phase === "extract") return `Installazione…${pct}`;
  return `Download…${pct} (${Math.round(t.done / MB)}${t.total ? ` di ${Math.round(t.total / MB)}` : ""} MB)`;
}

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

/** `onShow`: bring this view to the front (a file was opened from the Finder). */
export default function ProjectView({ active, onShow }: { active: boolean; onShow?: () => void }) {
  const [opened, setOpened] = useState<Omit<OpenedProject, "project"> | null>(null);
  const [project, setProjectRaw] = useState<ProjectData | null>(null);
  const [dirty, setDirty] = useState(false);
  const [singers, setSingers] = useState<SingersList | null>(null);
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [roll, setRoll] = useState<Roll | null>(null);
  const [tab, setTab] = useState<"prepara" | "studia" | "info">("prepara");
  // «Info»: what the project file and the cache weigh (read again after each save)
  const [sizes, setSizes] = useState<ProjectSizes | null>(null);
  const [cacheBytes, setCacheBytes] = useState<number | null>(null);
  const [infoTick, setInfoTick] = useState(0);
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

  const renderJobRef = useRef<number | null>(null);
  // reopening a project: its voices are read back from the cache when they are all there
  const [restoreWanted, setRestoreWanted] = useState(0);
  const [restoring, setRestoring] = useState(false);
  const restoreRef = useRef<{ job: number; sig: string } | null>(null);
  renderJobRef.current = renderJob;
  const playJobRef = useRef(0);
  // «Canta e controlla»
  const checkJobRef = useRef(0);
  const [checking, setChecking] = useState<CheckProgress | null>(null);
  const [checkResult, setCheckResult] = useState<CheckDone | null>(null);
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

  const [tiger, setTiger] = useState<TigerProgress | null>(null);
  const installTiger = useCallback(async () => {
    setError(null);
    setTiger({ phase: "download", done: 0, total: 0 });
    const un = await listen<TigerProgress>(TIGER_EVENT, (e) => setTiger(e.payload));
    try {
      const folder = await api.installTiger();
      await loadSingers();
      setNotice({ text: "Tiger è installato.", path: folder });
    } catch (e) {
      setError(errorText(e));
    } finally {
      un();
      setTiger(null);
    }
  }, [loadSingers]);

  const [sfInstalled, setSfInstalled] = useState(false);
  // the check is done: the voices of a reopened project can be looked up in the cache
  const [sfKnown, setSfKnown] = useState(false);
  const [sfProgress, setSfProgress] = useState<TigerProgress | null>(null);
  useEffect(() => {
    api
      .soundfontInstalled()
      .then(setSfInstalled, () => {})
      .finally(() => setSfKnown(true));
  }, []);
  const installSoundfont = useCallback(async () => {
    setError(null);
    setSfProgress({ phase: "download", done: 0, total: 0 });
    const un = await listen<TigerProgress>(SOUNDFONT_EVENT, (e) => setSfProgress(e.payload));
    try {
      await api.installSoundfont();
      setSfInstalled(true);
      setNotice({ text: "Strumenti installati: genera di nuovo l'audio per sentirli." });
    } catch (e) {
      setError(errorText(e));
    } finally {
      un();
      setSfProgress(null);
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
    setTab("prepara");
    restoreRef.current = null;
    setRestoring(false);
    setRestoreWanted((n) => n + 1);
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
        if (isProjectPath(path)) show(await projectApi.open(path), false);
        else if (isScorePath(path)) show(await projectApi.create(path), true);
        else throw "Apri una partitura MuseScore (.mscz) o MusicXML (.musicxml, .mxl) o un progetto Muse Sing (.msng)";
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
    projectApi.roll(opened.score_path, project.tempo_factor, project.marks ?? true, project.transpose ?? 0).then(
      (r) => !stale && setRoll(r),
      (e) => !stale && setError(errorText(e)),
    );
    return () => {
      stale = true;
    };
  }, [opened, project?.tempo_factor, project?.marks, project?.transpose]); // eslint-disable-line react-hooks/exhaustive-deps

  const activeRef = useRef(active);
  activeRef.current = active;
  const openRef = useRef(openPath);
  openRef.current = openPath;
  const showRef = useRef(show);
  showRef.current = show;
  const openedRef = useRef(opened);
  openedRef.current = opened;
  const onShowRef = useRef(onShow);
  onShowRef.current = onShow;

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
        const f = ev.payload.paths.find((p) => isProjectPath(p) || isScorePath(p)) ?? ev.payload.paths[0];
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
    // projects opened from the Finder (double click): at launch, and later while running
    const takeOpened = async () => {
      const files = await projectApi.takeOpened().catch(() => [] as string[]);
      const f = files.find((p) => isProjectPath(p) || isScorePath(p)) ?? files[0];
      if (!f) return;
      onShowRef.current?.();
      openRef.current(f);
    };
    // at launch with nothing opened from the Finder: the last project again, as it was left
    const reopenLast = async () => {
      const files = await projectApi.takeOpened().catch(() => [] as string[]);
      const f = files.find((p) => isProjectPath(p) || isScorePath(p)) ?? files[0];
      if (f) {
        onShowRef.current?.();
        return openRef.current(f);
      }
      const last = (await projectApi.recent().catch(() => [] as RecentProject[]))[0];
      if (!last || openedRef.current) return;
      try {
        const o = await projectApi.open(last.path);
        if (!openedRef.current) showRef.current(o, false);
      } catch {
        /* moved or deleted: the start page lists the others */
      }
    };
    // once now and once more when the listener is in place (a file may arrive in between)
    reopenLast();
    const unOpen = listen("open-files", takeOpened);
    unOpen.then(() => takeOpened());
    return () => {
      un.then((f) => f());
      unClose.then((f) => f());
      unOpen.then((f) => f());
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
        const r = restoreRef.current;
        if (r && d.job === r.job) {
          restoreRef.current = null;
          setRestoring(false);
          if (d.error || d.cancelled || renderJobRef.current !== null) return;
          setRendered({ sig: r.sig, seconds: d.seconds, warnings: d.warnings });
          if (sigRef.current === r.sig) setTab("studia");
          return;
        }
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
          // the voices are ready: on to studying (not after «Applica e ascolta» of the edits)
          if (then === null) setTab("studia");
          if (then !== null) window.setTimeout(() => startPlayRef.current(then, null), 50);
        }
      }),
      listen<PlayProgress>("sing-play", (e) => {
        if (e.payload.job === playJobRef.current) setPlay(e.payload);
      }),
      listen<CheckProgress>("sing-check", (e) => {
        if (e.payload.job === checkJobRef.current) setChecking(e.payload);
      }),
      listen<CheckDone>("sing-check-done", (e) => {
        const d = e.payload;
        if (d.job !== checkJobRef.current) return;
        checkJobRef.current = 0;
        setChecking(null);
        if (d.error === "") return;
        if (d.error) return setError(d.error);
        setCheckResult(d);
        setCursor(d.range[0]);
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
  // Studia's choices, kept in the project (older projects: the last «Io canto» of this computer)
  const mine = project ? (project.mine ?? readMine()) : null;
  const guideDb = project?.guide_db ?? -10;
  const ownVoice: OwnVoice = project?.own_voice ?? "muta";
  const setGuideDb = (db: number) => setProject((p) => ({ ...p, guide_db: db }));
  const setOwnVoice = (v: OwnVoice) => setProject((p) => ({ ...p, own_voice: v }));
  const parts = project?.parts ?? [];
  const sung = parts.filter((p) => p.sing);
  const singerOf = (p: PartCfg) => singers?.singers.find((s) => s.id === p.singer);
  const hasInstruments = !!analysis?.parts.some((p) => !p.sung);
  const accompOn = !!project?.accompaniment.enabled && hasInstruments;
  const voiced = sung.filter((p) => !p.played);
  const renderParts = sung.map((p) => ({ name: p.name, singer: p.singer, color: p.color, voices: p.voices ?? 1, played: !!p.played }));
  const instrumentParts = (analysis?.parts ?? []).filter((p) => !p.sung).map((p) => p.name);
  const instrumentOf = (name: string): Instrument => project?.instruments?.[name] ?? instrumentFor(name);
  const infoParts: InfoPart[] = (analysis?.parts ?? []).map((a) => {
    const p = parts.find((x) => x.name === a.name);
    if (!a.sung)
      return accompOn
        ? { name: a.name, how: "suonata", detail: sfInstalled ? INSTRUMENTS[instrumentOf(a.name)] : "organo semplice" }
        : { name: a.name, how: "non suonata", detail: "accompagnamento spento" };
    if (!p?.sing) return { name: a.name, how: "esclusa" };
    if (p.played) return { name: a.name, how: "suonata al pianoforte", detail: `${a.notes} note` };
    const who = [singerOf(p)?.name ?? "nessun cantante", p.color ? `colore ${p.color}` : null, (p.voices ?? 1) > 1 ? `${p.voices} coristi` : null];
    return { name: a.name, how: "cantata", detail: `${who.filter(Boolean).join(", ")} · ${a.notes} note` };
  });
  const renderInstruments: Record<string, Instrument> = Object.fromEntries(instrumentParts.map((n) => [n, instrumentOf(n)]));
  const renderEdits: Record<string, PartEdits> = Object.fromEntries(sung.filter((p) => project?.edits[p.name]).map((p) => [p.name, project!.edits[p.name]]));
  const sig =
    project && opened
      ? JSON.stringify([
          opened.score_path,
          project.lang,
          project.pitch,
          project.tempo_factor,
          accompOn,
          renderParts,
          renderEdits,
          renderInstruments,
          sfInstalled,
          project.marks ?? true,
          project.transpose ?? 0,
        ])
      : "";
  sigRef.current = sig;
  const upToDate = !!rendered && rendered.sig === sig;
  const audioState: AudioState = upToDate ? "pronte" : rendered ? "vecchie" : "nessuna";
  const mix: MixPart[] = useMemo(() => {
    if (!project) return [];
    const m: MixPart[] = project.parts.filter((p) => p.sing).map(({ name, volume_db, pan, mute, solo }) => ({ name, volume_db, pan, mute, solo }));
    const a = project.accompaniment;
    m.push({ name: ACCOMPANIMENT, volume_db: a.volume_db, pan: a.pan, mute: a.mute, solo: a.solo });
    const k = project.metronome ?? { enabled: false, volume_db: -6, pan: 0, mute: false, solo: false };
    m.push({ name: METRONOME, volume_db: k.volume_db, pan: k.pan, mute: k.mute || !k.enabled, solo: k.enabled && k.solo });
    return m;
  }, [project]);
  const ambience: Ambience = project?.ambience ?? "nessuno";
  const room: Room = { ambience, reverb: project?.reverb ?? 0.5 };
  const countIn = !!project?.count_in;
  const mixKey = JSON.stringify([mix, room, countIn]);
  const missingSinger = voiced.filter((p) => !singerOf(p));
  const notEngine = voiced.filter((p) => singerOf(p) && !singerOf(p)!.engine);
  const rendering = renderJob !== null;
  const diffsinger = (singers?.singers ?? []).filter((s) => s.singer_type === "diffsinger");
  const total = rendered?.seconds ?? 0;
  const pos = play ? play.t : checking ? checking.t : cursor;
  const nothing = sung.length === 0 && !accompOn;

  // --- actions -----------------------------------------------------------------------------------

  const newFromScore = async () => {
    const f = await open({ multiple: false, filters: [{ name: "Partitura MuseScore o MusicXML", extensions: SCORE_EXTENSIONS }] });
    if (typeof f === "string") openPath(f);
  };

  const openProject = async () => {
    const f = await open({ multiple: false, filters: [{ name: "Progetto Muse Sing", extensions: PROJECT_EXTENSIONS }] });
    if (typeof f === "string") openPath(f);
  };

  // the voices in memory when they match the choices, otherwise the ones already in the file
  const audioSave = (p: ProjectData): AudioSave => (!(p.keep_audio ?? true) ? "none" : upToDate ? "current" : "keep");

  const removeAudio = async () => {
    if (!project || !opened?.path) return;
    const ok = await ask("Le voci generate escono dal file del progetto, che viene salvato. Restano pronte finché l'app è aperta.", {
      title: "Rimuovere l'audio dal progetto?",
      kind: "warning",
      okLabel: "Rimuovi e salva",
      cancelLabel: "Annulla",
    });
    if (!ok) return;
    const p = { ...project, keep_audio: false };
    try {
      await projectApi.save(opened.path, p, opened.score_path, "none", opened.path);
      setProjectRaw(p);
      setDirty(false);
      setInfoTick((n) => n + 1);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const clearCache = async () => {
    const ok = await ask("Le voci dei progetti senza audio salvato andranno generate di nuovo.", {
      title: "Svuotare la cache delle voci?",
      kind: "warning",
      okLabel: "Svuota",
      cancelLabel: "Annulla",
    });
    if (!ok) return;
    await projectApi.cacheClear().catch((e) => setError(errorText(e)));
    setInfoTick((n) => n + 1);
  };

  useEffect(() => {
    if (tab !== "info") return;
    projectApi.cacheSize().then(setCacheBytes, () => setCacheBytes(null));
    if (opened?.path) projectApi.sizes(opened.path).then(setSizes, () => setSizes(null));
    else setSizes(null);
  }, [tab, opened?.path, infoTick]);

  const saveAs = async (forceDialog: boolean) => {
    if (!project || !opened) return;
    let path = opened.path;
    if (!path || forceDialog) {
      const picked = await save({
        defaultPath: join(opened.default_dir, `${fileSafe(project.title)}.${PROJECT_EXTENSION}`),
        filters: [{ name: "Progetto Muse Sing", extensions: [PROJECT_EXTENSION] }],
      });
      if (!picked) return;
      path = picked;
    }
    setError(null);
    try {
      const written = await projectApi.save(path, project, opened.score_path, audioSave(project), opened.path);
      setOpened({ ...opened, path: written });
      setDirty(false);
      setInfoTick((n) => n + 1);
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
    const f = await open({ multiple: false, filters: [{ name: "Partitura MuseScore o MusicXML", extensions: SCORE_EXTENSIONS }] });
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

  const renderRequest = (cacheOnly: boolean): RenderRequest | null =>
    project && opened
      ? {
          path: opened.score_path,
          lang: project.lang,
          pitch: project.pitch,
          parts: renderParts,
          tempo_factor: project.tempo_factor,
          accompaniment: accompOn,
          edits: renderEdits,
          instruments: renderInstruments,
          marks: project.marks ?? true,
          transpose: project.transpose ?? 0,
          cache_only: cacheOnly,
        }
      : null;

  // a project just opened: if its voices were generated before (and are still in the cache),
  // they are ready again without pressing Genera
  useEffect(() => {
    if (!restoreWanted || !sfKnown || !singers) return;
    const req = renderRequest(true);
    if (!req || rendered || renderJobRef.current !== null) return setRestoreWanted(0);
    // the singers of a new project are filled in just after it opens: wait for them
    if (nothing || missingSinger.length > 0 || notEngine.length > 0) return;
    setRestoreWanted(0);
    const s = sig;
    setRestoring(true);
    singApi.start(req).then(
      (job) => {
        if (renderJobRef.current === null) restoreRef.current = { job, sig: s };
        else setRestoring(false);
      },
      () => setRestoring(false),
    );
  }, [restoreWanted, sfKnown, singers, sig]); // eslint-disable-line react-hooks/exhaustive-deps

  const startRender = async (thenPlayFrom: number | null = null) => {
    const req = renderRequest(false);
    if (!req) return;
    playAfterRender.current = thenPlayFrom;
    setError(null);
    setNotice(null);
    restoreRef.current = null;
    setRestoring(false);
    if (play) await singApi.stop();
    try {
      const job = await singApi.start(req);
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
        const [m, rm, ci] = JSON.parse(mixKey) as [MixPart[], Room, boolean];
        const job = await singApi.play(m, from, range, rm, ci);
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

  // --- «Canta e controlla» ---
  const mineSung = sung.some((p) => p.name === mine) ? mine : null;
  // the result belongs to this render: a new tempo, key or edit moves the notes
  useEffect(() => setCheckResult(null), [sig, mineSung]);
  /** The mix with your own part muted or softer. */
  const checkMix = (): MixPart[] =>
    mix.map((p) => {
      if (p.name !== mineSung) return { ...p, solo: false };
      const db = OWN_VOICE_DB[ownVoice];
      return db === null ? { ...p, mute: true, solo: false } : { ...p, volume_db: p.volume_db + db, solo: false };
    });
  const barNumberAt = (sec: number) => {
    const b = (roll?.bars ?? []).filter((x) => x.start <= sec + 1e-6).pop();
    return b?.number ?? null;
  };
  const stretch = (() => {
    const [a, b] = loop ?? [cursor, null];
    const na = barNumberAt(a);
    const from = a < 0.05 ? "dall'inizio" : na ? `dalla battuta ${na}` : `da ${fmtDuration(a)}`;
    const nb = b === null ? null : barNumberAt(b);
    return `${from} ${b === null ? "alla fine" : nb ? `alla battuta ${nb}` : `a ${fmtDuration(b)}`}`;
  })();
  const startCheck = async () => {
    if (!mineSung) return;
    setError(null);
    try {
      if (play) await singApi.stop();
      const [a, b] = loop ?? [cursor, null];
      const job = await singApi.check(checkMix(), mineSung, a, b, room);
      checkJobRef.current = job;
      setChecking({ job, t: a, counting: true, analyzing: false, level: 0 });
    } catch (e) {
      setError(errorText(e));
    }
  };
  const replayCheck = async () => {
    setError(null);
    try {
      const job = await singApi.checkReplay(checkMix(), room);
      playJobRef.current = job;
      setPlay({ job, t: checkResult?.range[0] ?? 0, total: rendered?.seconds ?? 0 });
    } catch (e) {
      setError(errorText(e));
    }
  };
  const checkSummary = checkResult
    ? summarize(
        checkResult,
        roll?.parts.find((p) => p.name === checkResult.part),
        roll?.bars ?? [],
      )
    : null;

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
      const written = await singApi.export(mix, path, room);
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
      const files = await singApi.exportGuides(mix, dir, "mp3", guideDb, room, countIn, project?.guides ?? "evidenza");
      setNotice({ text: `Basi di studio: ${files.length} file MP3 in ${baseName(dir)}`, path: files[0] });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setExporting(false);
    }
  };

  const chooseMine = (name: string | null) => {
    setProject((p) => ({ ...p, mine: name }));
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
          Rilascia qui una partitura (.mscz, .musicxml, .mxl) o un progetto (.msng)
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
              {dirty && (
                <span className="ml-1 text-violet-600" title="Modifiche non salvate">
                  ●
                </span>
              )}
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
            <span className="text-base font-medium">Trascina qui una partitura MuseScore (.mscz) o MusicXML, o un progetto Muse Sing (.msng)</span>
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
                {analysis.file_name} · {analysis.time_sig} · {/^(6|9|12)\/8\b/.test(analysis.time_sig) ? "♩." : "♩"} = {analysis.bpm} ·{" "}
                {fmtDuration(analysis.duration_sec)}
              </div>
            </div>
            <button
              className="btn"
              onClick={reloadScore}
              disabled={busy || rendering}
              title="Sostituisci la partitura con una versione aggiornata, tenendo le scelte"
            >
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

          <div className="flex gap-1 border-b border-zinc-200 dark:border-zinc-800" role="tablist">
            {(
              [
                ["prepara", "1 · Prepara le voci", "Lingua, cantanti, strumenti e ritocchi: tutto ciò che serve a generare l'audio"],
                ["studia", "2 · Studia", "Mixer, ascolto, Canta e controlla, basi di studio"],
                ["info", "Info", "Il brano, le dimensioni del progetto e della cache, l'audio salvato"],
              ] as const
            ).map(([k, label, title]) => (
              <button
                key={k}
                role="tab"
                aria-selected={tab === k}
                title={k === "studia" && !rendered ? "Genera prima l'audio" : title}
                disabled={k === "studia" && !rendered}
                onClick={() => setTab(k)}
                className={`-mb-px border-b-2 px-4 py-2 text-sm font-medium disabled:opacity-40 ${k === "info" ? "ml-auto" : ""} ${tab === k ? "border-violet-600 text-violet-700 dark:text-violet-300" : "border-transparent text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200"}`}
              >
                {label}
              </button>
            ))}
          </div>

          {tab === "prepara" ? (
            <>
              <section className="card grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                <div>
                  <label className="label">Lingua del testo</label>
                  <select
                    className="field"
                    aria-label="Lingua del testo"
                    value={project.lang}
                    onChange={(e) => setProject((p) => ({ ...p, lang: e.target.value as Lang }))}
                    disabled={rendering}
                  >
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
                  <select
                    className="field"
                    aria-label="Intonazione"
                    value={project.pitch}
                    onChange={(e) => setProject((p) => ({ ...p, pitch: e.target.value as PitchMode }))}
                    disabled={rendering}
                  >
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
                  <label
                    className="mt-2 flex items-center gap-1.5 text-xs text-zinc-500"
                    title="Dinamiche e forcelle, respiri, corone, rit. e accel. scritti nella partitura. Togli la spunta per un'esecuzione piatta e a tempo costante."
                  >
                    <input
                      type="checkbox"
                      className="accent-violet-600"
                      checked={project.marks ?? true}
                      onChange={() => setProject((p) => ({ ...p, marks: !(p.marks ?? true) }))}
                      disabled={rendering}
                    />
                    segni della partitura
                  </label>
                </div>
                <div>
                  <label className="label">Tonalità</label>
                  <select
                    className="field"
                    aria-label="Tonalità"
                    title="Sposta tutto (voci, strumenti, rullo) di qualche semitono, per adattarlo alla tua voce"
                    value={project.transpose ?? 0}
                    onChange={(e) => setProject((p) => ({ ...p, transpose: Number(e.target.value) }))}
                    disabled={rendering}
                  >
                    {Array.from({ length: 2 * MAX_TRANSPOSE + 1 }, (_, i) => MAX_TRANSPOSE - i).map((t) => (
                      <option key={t} value={t}>
                        {t === 0
                          ? "Come scritta"
                          : `${t > 0 ? "+" : "−"}${Math.abs(t)} ${Math.abs(t) === 1 ? "semitono" : "semitoni"} ${t > 0 ? "sopra" : "sotto"}`}
                      </option>
                    ))}
                  </select>
                </div>
              </section>
              <section className="card">
                <div className="label flex items-center justify-between">
                  <span>Voci e cantanti</span>
                  <button className="text-violet-600 hover:underline" onClick={loadSingers} title="Rileggi i cantanti installati" disabled={rendering}>
                    <RefreshCw className="inline h-3 w-3" /> aggiorna cantanti
                  </button>
                </div>
                {singers && diffsinger.length === 0 && (
                  <div className="mb-2 rounded-md border border-zinc-300 p-2 text-zinc-500 dark:border-zinc-700">
                    <p>
                      Nessun cantante installato. Muse Sing usa <b>TIGER DS</b> (di tigermeat), che canta in italiano, latino e inglese: si scarica gratis dal
                      sito dell'autore (circa 550 MB), solo per uso non commerciale e citando l'autore.
                    </p>
                    <div className="mt-2 flex items-center gap-3">
                      <button className="btn-primary" onClick={installTiger} disabled={!!tiger || rendering}>
                        {tiger ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />} Scarica Tiger
                      </button>
                      {tiger && <span className="text-xs">{tigerText(tiger)}</span>}
                    </div>
                    <p className="mt-2 text-xs">
                      Oppure copia un voicebank DiffSinger in una di queste cartelle e premi «aggiorna cantanti»:{" "}
                      {[...singers.folders, singers.own_folder].filter(Boolean).join(", ")}.
                    </p>
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
                            </div>
                            {info && <div className="truncate text-xs italic text-zinc-500">{info.preview.replace(/\[[^\]]*\]/g, "")} …</div>}
                          </div>
                          <div className="flex shrink-0 gap-1.5">
                            <select
                              className="field w-28 py-1"
                              value={p.played ? "suona" : "canta"}
                              disabled={!p.sing || rendering}
                              onChange={(e) => setPart(p.name, { played: e.target.value === "suona" })}
                              title="«Suonata»: la voce è suonata al pianoforte invece che cantata, per imparare le note"
                              aria-label={`Cantata o suonata: ${p.name}`}
                            >
                              <option value="canta">Cantata</option>
                              <option value="suona">Suonata</option>
                            </select>
                            {!p.played && (
                              <>
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
                                <select
                                  className="field w-28 py-1"
                                  value={p.voices ?? 1}
                                  disabled={!p.sing || rendering}
                                  onChange={(e) => setPart(p.name, { voices: Number(e.target.value) })}
                                  title="Più coristi sulla stessa voce, ognuno con un colore diverso: suona come una sezione del coro. La generazione dura di più."
                                  aria-label={`Coristi di ${p.name}`}
                                >
                                  <option value={1}>1 corista</option>
                                  <option value={2}>2 coristi</option>
                                  <option value={3}>3 coristi</option>
                                </select>
                              </>
                            )}
                            {p.played && <span className="self-center px-2 text-xs text-zinc-500">al pianoforte{sfInstalled ? "" : " (suono semplice)"}</span>}
                          </div>
                        </div>
                        {p.sing && s && !s.engine && <div className="mt-1 pl-7 text-xs text-amber-600">{s.engine_note}</div>}
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
                              · {sfInstalled ? "le parti strumentali" : `${instrumentParts.join(", ")} con suono d'organo semplice`}
                            </span>
                          </div>
                          {sfInstalled ? (
                            project.accompaniment.enabled && (
                              <div className="mt-1.5 flex flex-wrap gap-2">
                                {instrumentParts.map((name) => (
                                  <label key={name} className="flex items-center gap-1.5 text-xs text-zinc-500">
                                    {name}
                                    <select
                                      className="field w-40 py-0.5"
                                      value={instrumentOf(name)}
                                      disabled={rendering}
                                      onChange={(e) =>
                                        setProject((p) => ({ ...p, instruments: { ...(p.instruments ?? {}), [name]: e.target.value as Instrument } }))
                                      }
                                      aria-label={`Strumento di ${name}`}
                                    >
                                      {Object.entries(INSTRUMENTS).map(([k, v]) => (
                                        <option key={k} value={k}>
                                          {v}
                                        </option>
                                      ))}
                                    </select>
                                  </label>
                                ))}
                              </div>
                            )
                          ) : (
                            <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-zinc-500">
                              <button className="btn py-0.5 text-xs" onClick={installSoundfont} disabled={!!sfProgress || rendering}>
                                {sfProgress ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Download className="h-3.5 w-3.5" />} Scarica strumenti veri
                              </button>
                              {sfProgress ? tigerText(sfProgress) : "organo a canne, pianoforte, archi… (31 MB, GeneralUser GS)"}
                            </div>
                          )}
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              </section>
              <section className="card space-y-3">
                <div className="flex items-center gap-2 font-semibold">
                  <AudioLines className="h-4 w-4 text-violet-600" /> Genera le voci
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
                    <button
                      className="btn-primary"
                      onClick={() => startRender()}
                      disabled={busy || nothing || missingSinger.length > 0 || notEngine.length > 0}
                    >
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
                    ) : restoring ? (
                      <span className="flex items-center gap-2 text-zinc-500">
                        <Loader2 className="h-3.5 w-3.5 animate-spin" /> Ricarico le voci già generate…
                      </span>
                    ) : !rendered ? (
                      <span className="text-zinc-500">Le voci già generate con le stesse scelte si riusano subito.</span>
                    ) : null}
                  </div>
                )}
                {rendered && upToDate && !rendering && (
                  <div className="flex flex-wrap items-center gap-3 text-emerald-700 dark:text-emerald-300">
                    <CheckCircle2 className="h-4 w-4" /> Voci pronte.
                    <button className="btn" onClick={() => setTab("studia")}>
                      Vai a Studia →
                    </button>
                  </div>
                )}
                {rendered && rendered.warnings.length > 0 && (
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
              </section>
              {roll && rendered && sung.length > 0 && (
                <section className="card">
                  <Editor
                    parts={voiced.map((p) => p.name)}
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
          ) : tab === "studia" ? (
            <>
              {(!upToDate || rendering) && (
                <section className="card space-y-3 border-amber-300 dark:border-amber-800">
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
                      <button
                        className="btn-primary"
                        onClick={() => startRender()}
                        disabled={busy || nothing || missingSinger.length > 0 || notEngine.length > 0}
                      >
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
                </section>
              )}
              <section className="card flex flex-wrap items-center gap-3">
                <label className="label mb-0" htmlFor="io-canto">
                  Io canto
                </label>
                <select id="io-canto" className="field w-56" value={mineSung ?? ""} onChange={(e) => chooseMine(e.target.value || null)} disabled={!!checking}>
                  <option value="">— nessuna voce —</option>
                  {sung.map((p) => (
                    <option key={p.name} value={p.name}>
                      {p.name}
                    </option>
                  ))}
                </select>
                <span className="text-xs text-zinc-500">La tua voce è evidenziata nel rullo ed è quella di Canta e controlla.</span>
              </section>
              <Mixer
                parts={sung}
                mine={mineSung}
                accompaniment={accompOn ? project.accompaniment : null}
                metronome={project.metronome ?? { enabled: false, volume_db: -6, pan: 0, mute: false, solo: false }}
                ambience={ambience}
                reverb={room.reverb}
                onPart={(name, patch) => setPart(name, patch)}
                onAccompaniment={(patch) => setProject((p) => ({ ...p, accompaniment: { ...p.accompaniment, ...patch } }))}
                onMetronome={(patch) => setProject((p) => ({ ...p, metronome: { ...p.metronome, ...patch } }))}
                onAmbience={(a) => setProject((p) => ({ ...p, ambience: a }))}
                onReverb={(r) => setProject((p) => ({ ...p, reverb: r }))}
                disabled={!!checking}
              />
              {rendered && upToDate && !rendering && (
                <section className="card space-y-3">
                  <div className="flex flex-wrap items-center gap-3">
                    <button
                      className="btn"
                      onClick={() => (play ? stopPlay() : startPlay(loop && (cursor < loop[0] || cursor > loop[1]) ? loop[0] : cursor, loop))}
                    >
                      {play ? <Square className="h-4 w-4" /> : <Play className="h-4 w-4" />} {play ? "Ferma" : "Ascolta"}
                    </button>
                    <label
                      className="flex items-center gap-1.5 text-xs text-zinc-500"
                      title="Una battuta di colpi prima di partire, con l'accordo di partenza (non quando ripeti un tratto)"
                    >
                      <input
                        type="checkbox"
                        className="accent-violet-600"
                        checked={countIn}
                        onChange={() => setProject((p) => ({ ...p, count_in: !p.count_in }))}
                      />
                      conteggio
                    </label>
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
                </section>
              )}
              {roll && roll.parts.length > 0 && (
                <section className="card">
                  <ScoreRoll
                    parts={roll.parts}
                    bars={roll.bars ?? []}
                    cursor={pos}
                    playing={!!play || !!checking}
                    highlight={mine}
                    loop={loop}
                    check={
                      checkResult && {
                        part: checkResult.part,
                        verdicts: Object.fromEntries(checkResult.notes.map((n) => [n.id, n.verdict])),
                        line: checkResult.line,
                      }
                    }
                    onSeek={seek}
                    onLoop={(r) => {
                      setLoop(r);
                      if (r) setCursor(r[0]);
                    }}
                  />
                </section>
              )}
              {roll && rendered && upToDate && sung.length > 0 && (
                <CheckPanel
                  mine={mineSung}
                  ownVoice={ownVoice}
                  onOwnVoice={setOwnVoice}
                  checking={checking ? (checking.analyzing ? "analyzing" : checking.counting ? "counting" : "singing") : null}
                  level={checking?.level ?? 0}
                  result={checkResult}
                  summary={checkSummary}
                  stretch={stretch}
                  onStart={startCheck}
                  onStop={() => singApi.stop()}
                  onReplay={replayCheck}
                  onClear={() => setCheckResult(null)}
              active={active}
                  disabled={rendering || exporting}
                />
              )}
              {rendered && upToDate && !rendering && (
                <section className="card space-y-3">
                  <div className="label">Esporta</div>
                  <div className="flex flex-wrap items-center gap-3">
                    <button className="btn" onClick={exportAudio} disabled={exporting}>
                      {exporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />} Esporta…
                    </button>
                    <button
                      className="btn"
                      onClick={exportGuides}
                      disabled={exporting || sung.length < 2}
                      title="Un MP3 per ogni voce: con quella voce in evidenza, oppure senza (per cantarci sopra)"
                    >
                      <Headphones className="h-4 w-4" /> Basi di studio…
                    </button>
                    <select
                      className="field w-auto py-0.5 text-xs"
                      aria-label="Tipo di basi"
                      value={project.guides ?? "evidenza"}
                      onChange={(e) => setProject((p) => ({ ...p, guides: e.target.value as GuideKind }))}
                    >
                      <option value="evidenza">voce in evidenza</option>
                      <option value="senza">senza la propria voce</option>
                      <option value="entrambe">entrambe</option>
                    </select>
                    <label className={`flex items-center gap-2 text-xs text-zinc-500 ${(project.guides ?? "evidenza") === "senza" ? "hidden" : ""}`}>
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
              )}
            </>
          ) : (
            <Info
              heading={`${analysis.file_name} · ${analysis.time_sig} · ${/^(6|9|12)\/8\b/.test(analysis.time_sig) ? "♩." : "♩"} = ${analysis.bpm} · ${fmtDuration(analysis.duration_sec)}`}
              parts={infoParts}
              settings={[
                ["Lingua", LANGS[project.lang]],
                ["Intonazione", project.pitch === "notes" ? "esatta (ferma sulle note)" : "naturale (decide il cantante)"],
                ["Velocità", `${Math.round(project.tempo_factor * 100)}%`],
                ["Tonalità", project.transpose ? `${project.transpose > 0 ? "+" : ""}${project.transpose} semitoni` : "come scritta"],
                ["Segni della partitura", (project.marks ?? true) ? "seguiti" : "ignorati"],
                ["Ambiente", `${AMBIENCES[ambience]}${ambience !== "nessuno" ? `, riverbero ${Math.round((project.reverb ?? 0.5) * 100)}%` : ""}`],
              ]}
              path={opened?.path ?? null}
              dirty={dirty}
              sizes={sizes}
              audio={audioState}
              keepAudio={project.keep_audio ?? true}
              onKeepAudio={(v) => setProject((p) => ({ ...p, keep_audio: v }))}
              onRemoveAudio={removeAudio}
              cacheBytes={cacheBytes}
              onClearCache={clearCache}
              busy={busy || rendering}
            />
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
        L'audio è generato sul Mac con il motore DiffSinger di Muse Sing (derivato da OpenUtau). TIGER DS (tigermeat) è gratuito solo per uso non commerciale,
        citando l'autore.
      </footer>
    </main>
  );
}
