import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  AudioLines,
  CheckCircle2,
  Download,
  FileMusic,
  FolderOpen,
  Loader2,
  Mic2,
  Play,
  RefreshCw,
  Square,
  Upload,
  Volume2,
  VolumeX,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  errorText,
  LANGS,
  singApi,
  type Analysis,
  type GenerateResult,
  type Lang,
  type MixPart,
  type PitchMode,
  type PlayDone,
  type PlayProgress,
  type SingDone,
  type SingerInfo,
  type SingersList,
  type SingProgress,
} from "../lib/api";

function fmtDuration(sec: number) {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

/** Choice and mix settings of one part. */
type PartCfg = { singer: string; color: string | null; volume_db: number; pan: number; mute: boolean };

const defaultCfg = (singer: string, color: string | null = null): PartCfg => ({ singer, color, volume_db: 0, pan: 0, mute: false });

function panLabel(p: number) {
  if (Math.abs(p) < 0.05) return "C";
  return `${p < 0 ? "S" : "D"} ${Math.round(Math.abs(p) * 100)}`;
}

function singerLabel(s: SingerInfo, lang: Lang) {
  const notes = [];
  if (!s.engine) notes.push("solo .ustx");
  else if (!s.openutau) notes.push("solo Muse Sing");
  if (!s.langs.includes(lang)) notes.push(`senza dizionario ${lang}`);
  return notes.length ? `${s.name} (${notes.join(", ")})` : s.name;
}

export default function Convert({ active }: { active: boolean }) {
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [lang, setLang] = useState<Lang>("it");
  const [selectedParts, setSelectedParts] = useState<string[]>([]);
  const [singers, setSingers] = useState<SingersList | null>(null);
  const [cfg, setCfg] = useState<Record<string, PartCfg>>({});
  const [pitch, setPitch] = useState<PitchMode>("model");
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<GenerateResult | null>(null);

  // render
  const [renderJob, setRenderJob] = useState<number | null>(null);
  const [progress, setProgress] = useState<SingProgress | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [rendered, setRendered] = useState<{ sig: string; seconds: number; warnings: string[] } | null>(null);
  const [showWarnings, setShowWarnings] = useState(false);
  // playback and export
  const [play, setPlay] = useState<{ job: number; t: number; total: number } | null>(null);
  const [cursor, setCursor] = useState(0);
  const [exported, setExported] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  const renderJobRef = useRef<number | null>(null);
  renderJobRef.current = renderJob;
  const playJobRef = useRef(0);
  const playingRef = useRef(play);
  playingRef.current = play;
  const seekTimer = useRef<number | undefined>(undefined);

  const loadSingers = useCallback(async () => {
    try {
      setSingers(await api.listSingers());
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  const loadFile = useCallback(async (path: string) => {
    setError(null);
    setResult(null);
    setRendered(null);
    setExported(null);
    setBusy(true);
    try {
      const a = await api.analyze(path);
      setAnalysis(a);
      setLang(a.lang);
      setSelectedParts(a.parts.filter((p) => p.sung).map((p) => p.name));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }, []);

  // a singer for every sung part: kept if still installed, else the saved one for that part name,
  // else the language's default
  useEffect(() => {
    if (!analysis || !singers) return;
    setCfg((cur) => {
      const next: Record<string, PartCfg> = {};
      const fallback = singers.selected[lang] ?? "";
      for (const p of analysis.parts.filter((p) => p.sung)) {
        const old = cur[p.name];
        if (old && singers.singers.some((s) => s.id === old.singer)) {
          next[p.name] = old;
          continue;
        }
        const saved = singers.parts[p.name.toLowerCase()];
        next[p.name] = { ...(old ?? defaultCfg("")), ...(saved ? { singer: saved.singer, color: saved.color } : { singer: fallback, color: null }) };
      }
      return next;
    });
  }, [analysis, singers, lang]);

  const activeRef = useRef(active);
  activeRef.current = active;

  useEffect(() => {
    loadSingers();
    const un = getCurrentWebview().onDragDropEvent((ev) => {
      if (!activeRef.current) return;
      const t = ev.payload.type;
      if (t === "enter" || t === "over") setDragging(true);
      else if (t === "leave") setDragging(false);
      else if (t === "drop") {
        setDragging(false);
        const f = ev.payload.paths.find((p) => p.toLowerCase().endsWith(".mscz")) ?? ev.payload.paths[0];
        if (f) loadFile(f);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [loadFile, loadSingers]);

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
        if (d.error) setError(d.error);
        else if (!d.cancelled) {
          setRendered({ sig: sigRef.current, seconds: d.seconds, warnings: d.warnings });
          setCursor(0);
        }
      }),
      listen<PlayProgress>("sing-play", (e) => {
        if (e.payload.job === playJobRef.current) setPlay(e.payload);
      }),
      listen<PlayDone>("sing-play-done", (e) => {
        if (e.payload.job !== playJobRef.current) return;
        // stopped by the user: resume from there next time; at the end: from the top
        const p = playingRef.current;
        setCursor(p && p.total - p.t > 0.3 ? p.t : 0);
        setPlay(null);
        if (e.payload.error) setError(e.payload.error);
      }),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);

  // --- derived -----------------------------------------------------------------------------------

  const sungSelected = useMemo(
    () => (analysis?.parts ?? []).filter((p) => p.sung && selectedParts.includes(p.name)).map((p) => p.name),
    [analysis, selectedParts],
  );
  const singerOf = (name: string) => singers?.singers.find((s) => s.id === cfg[name]?.singer);
  const renderParts = sungSelected.map((name) => ({ name, singer: cfg[name]?.singer ?? "", color: cfg[name]?.color ?? null }));
  const sig = analysis ? JSON.stringify({ path: analysis.path, lang, pitch, parts: renderParts }) : "";
  sigRef.current = sig;
  const upToDate = !!rendered && rendered.sig === sig;
  const mix: MixPart[] = sungSelected.map((name) => {
    const c = cfg[name] ?? defaultCfg("");
    return { name, volume_db: c.volume_db, pan: c.pan, mute: c.mute };
  });
  const mixKey = JSON.stringify(mix);

  const missingSinger = sungSelected.filter((n) => !singerOf(n));
  const notEngine = sungSelected.filter((n) => singerOf(n) && !singerOf(n)!.engine);
  const notOpenUtau = sungSelected.filter((n) => singerOf(n) && !singerOf(n)!.openutau);
  const rendering = renderJob !== null;
  const diffsinger = (singers?.singers ?? []).filter((s) => s.singer_type === "diffsinger");

  // --- actions -----------------------------------------------------------------------------------

  const pickFile = async () => {
    const f = await open({ multiple: false, filters: [{ name: "MuseScore", extensions: ["mscz"] }] });
    if (typeof f === "string") loadFile(f);
  };

  const setPart = (name: string, patch: Partial<PartCfg>) => setCfg((cur) => ({ ...cur, [name]: { ...(cur[name] ?? defaultCfg("")), ...patch } }));

  const togglePart = (name: string) => setSelectedParts((cur) => (cur.includes(name) ? cur.filter((n) => n !== name) : [...cur, name]));

  const startRender = async () => {
    if (!analysis) return;
    setError(null);
    setExported(null);
    if (play) await singApi.stop();
    try {
      const job = await singApi.start({ path: analysis.path, lang, pitch, parts: renderParts });
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
    async (from: number) => {
      setError(null);
      try {
        const job = await singApi.play(JSON.parse(mixKey) as MixPart[], from);
        playJobRef.current = job;
        setPlay({ job, t: from, total: rendered?.seconds ?? 0 });
      } catch (e) {
        setError(errorText(e));
      }
    },
    [mixKey, rendered],
  );

  const stopPlay = () => singApi.stop();

  // volume / pan / mute changed while playing: start again from the same point with the new mix
  const firstMix = useRef(true);
  useEffect(() => {
    if (firstMix.current) {
      firstMix.current = false;
      return;
    }
    if (!playingRef.current) return;
    const t = window.setTimeout(() => {
      const p = playingRef.current;
      if (p) startPlay(p.t);
    }, 250);
    return () => clearTimeout(t);
  }, [mixKey, startPlay]);

  const exportAudio = async () => {
    if (!analysis) return;
    setError(null);
    const out = await save({
      defaultPath: analysis.default_output.replace(/\.ustx$/i, ".mp3"),
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
      setExported(await singApi.export(mix, path));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setExporting(false);
    }
  };

  const generateUstx = async () => {
    if (!analysis) return;
    setError(null);
    setResult(null);
    const output = await save({ defaultPath: analysis.default_output, filters: [{ name: "Progetto OpenUtau", extensions: ["ustx"] }] });
    if (!output) return;
    setBusy(true);
    try {
      const part_singers = Object.fromEntries(sungSelected.map((n) => [n, cfg[n]?.singer ?? ""]));
      setResult(await api.generate({ path: analysis.path, output, lang, singer: part_singers[sungSelected[0]] ?? "", parts: sungSelected, part_singers }));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  // --- view --------------------------------------------------------------------------------------

  const renderPct = progress ? ((progress.index + (progress.phrases ? progress.phrase / progress.phrases : 0)) / Math.max(1, progress.parts)) * 100 : 0;
  const total = rendered?.seconds ?? 0;
  const pos = play ? play.t : cursor;

  return (
    <>
      <main className={`relative flex-1 space-y-4 overflow-auto p-5 ${active ? "" : "hidden"}`}>
        {dragging && (
          <div className="pointer-events-none absolute inset-3 z-10 flex items-center justify-center rounded-2xl border-2 border-dashed border-violet-500 bg-violet-500/10 text-lg font-medium text-violet-700 dark:text-violet-300">
            Rilascia qui il file MuseScore
          </div>
        )}

        {!analysis ? (
          <button
            onClick={pickFile}
            className="flex w-full flex-col items-center justify-center gap-3 rounded-2xl border-2 border-dashed border-zinc-300 py-16 text-zinc-500 hover:border-violet-400 hover:text-violet-600 dark:border-zinc-700"
          >
            {busy ? <Loader2 className="h-8 w-8 animate-spin" /> : <Upload className="h-8 w-8" />}
            <span className="text-base font-medium">Trascina qui un file MuseScore (.mscz)</span>
            <span>oppure clicca per sceglierlo</span>
          </button>
        ) : (
          <>
            <section className="card flex items-start gap-3">
              <FileMusic className="mt-0.5 h-6 w-6 shrink-0 text-violet-600" />
              <div className="min-w-0 flex-1">
                <div className="text-base font-semibold">{analysis.title}</div>
                <div className="truncate text-zinc-500" title={analysis.path}>
                  {analysis.file_name} · {analysis.time_sig} · {/^(6|9|12)\/8$/.test(analysis.time_sig) ? "♩." : "♩"} = {analysis.bpm} ·{" "}
                  {fmtDuration(analysis.duration_sec)}
                </div>
              </div>
              <button className="btn" onClick={pickFile} disabled={busy || rendering}>
                <FolderOpen className="h-4 w-4" /> Altro file
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

            <section className="card grid gap-4 sm:grid-cols-2">
              <div>
                <label className="label">Lingua del testo</label>
                <select className="field" value={lang} onChange={(e) => setLang(e.target.value as Lang)} disabled={rendering}>
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
                <select className="field" value={pitch} onChange={(e) => setPitch(e.target.value as PitchMode)} disabled={rendering}>
                  <option value="model">Naturale (decide il cantante)</option>
                  <option value="notes">Esatta (ferma sulle note)</option>
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
                  Nessun cantante DiffSinger trovato in {[...singers.folders, singers.own_folder].filter(Boolean).join(", ")}. Installane uno in
                  OpenUtau (o copialo nella cartella di Muse Sing) e premi «aggiorna cantanti».
                </div>
              )}
              <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
                {analysis.parts.map((p) => {
                  const c = cfg[p.name] ?? defaultCfg("");
                  const s = singerOf(p.name);
                  const on = selectedParts.includes(p.name);
                  return (
                    <div key={p.name} className={`py-2.5 ${p.sung ? "" : "opacity-50"}`}>
                      <div className="flex items-start gap-3">
                        <input
                          type="checkbox"
                          className="mt-1 accent-violet-600"
                          disabled={!p.sung || rendering}
                          checked={on}
                          onChange={() => togglePart(p.name)}
                          aria-label={`Canta ${p.name}`}
                        />
                        <div className="min-w-0 flex-1">
                          <div className="font-medium">
                            {p.name}{" "}
                            <span className="font-normal text-zinc-500">
                              · {p.notes} note{p.sung ? ` · ${p.syllables} sillabe` : " · senza testo (strumento)"}
                            </span>
                          </div>
                          {p.sung && <div className="truncate text-xs italic text-zinc-500">{p.preview.replace(/\[[^\]]*\]/g, "")} …</div>}
                        </div>
                        {p.sung && (
                          <div className="flex shrink-0 gap-1.5">
                            <select
                              className="field w-52 py-1"
                              value={c.singer}
                              disabled={!on || rendering}
                              onChange={(e) => setPart(p.name, { singer: e.target.value, color: null })}
                              aria-label={`Cantante di ${p.name}`}
                            >
                              <option value="">— cantante —</option>
                              {diffsinger.map((x) => (
                                <option key={x.id} value={x.id}>
                                  {singerLabel(x, lang)}
                                </option>
                              ))}
                            </select>
                            {s && s.colors.length > 1 && (
                              <select
                                className="field w-32 py-1"
                                value={c.color ?? ""}
                                disabled={!on || rendering}
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
                        )}
                      </div>
                      {p.sung && on && (
                        <div className="mt-1.5 flex flex-wrap items-center gap-x-5 gap-y-1 pl-7 text-xs text-zinc-500">
                          <label className="flex items-center gap-2">
                            Volume
                            <input
                              type="range"
                              min={-24}
                              max={6}
                              step={1}
                              value={c.volume_db}
                              onChange={(e) => setPart(p.name, { volume_db: Number(e.target.value) })}
                              onDoubleClick={() => setPart(p.name, { volume_db: 0 })}
                              className="w-28 accent-violet-600"
                            />
                            <span className="w-12 tabular-nums">
                              {c.volume_db > 0 ? "+" : ""}
                              {c.volume_db} dB
                            </span>
                          </label>
                          <label className="flex items-center gap-2">
                            Pan
                            <input
                              type="range"
                              min={-1}
                              max={1}
                              step={0.05}
                              value={c.pan}
                              onChange={(e) => setPart(p.name, { pan: Number(e.target.value) })}
                              onDoubleClick={() => setPart(p.name, { pan: 0 })}
                              className="w-24 accent-violet-600"
                            />
                            <span className="w-10 tabular-nums">{panLabel(c.pan)}</span>
                          </label>
                          <button
                            className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 ${c.mute ? "bg-zinc-200 text-zinc-800 dark:bg-zinc-700 dark:text-zinc-100" : "hover:text-zinc-800 dark:hover:text-zinc-200"}`}
                            onClick={() => setPart(p.name, { mute: !c.mute })}
                            title="Escludi dall'ascolto e dall'export (senza rigenerare)"
                          >
                            {c.mute ? <VolumeX className="h-3.5 w-3.5" /> : <Volume2 className="h-3.5 w-3.5" />}
                            {c.mute ? "muta" : "attiva"}
                          </button>
                          {s && !s.engine && <span className="text-amber-600">{s.engine_note}</span>}
                        </div>
                      )}
                    </div>
                  );
                })}
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
                  <button
                    className="btn-primary"
                    onClick={startRender}
                    disabled={busy || sungSelected.length === 0 || missingSinger.length > 0 || notEngine.length > 0}
                  >
                    <Mic2 className="h-4 w-4" /> {rendered ? "Genera di nuovo" : "Genera audio"}
                  </button>
                  {sungSelected.length === 0 ? (
                    <span className="text-zinc-500">Scegli almeno una voce.</span>
                  ) : missingSinger.length > 0 ? (
                    <span className="text-zinc-500">Scegli il cantante per: {missingSinger.join(", ")}.</span>
                  ) : notEngine.length > 0 ? (
                    <span className="text-amber-600">
                      {notEngine.join(", ")}: {singerOf(notEngine[0])?.name} si usa solo nel .ustx con OpenUtau. Scegli un altro cantante per generare
                      l'audio qui.
                    </span>
                  ) : rendered && !upToDate ? (
                    <span className="text-amber-600">Hai cambiato voci, cantanti, lingua o intonazione: genera di nuovo.</span>
                  ) : !rendered ? (
                    <span className="text-zinc-500">Le voci già generate con gli stessi cantanti si riusano subito.</span>
                  ) : null}
                </div>
              )}

              {rendered && upToDate && !rendering && (
                <>
                  <div className="flex items-center gap-3">
                    <button className="btn" onClick={() => (play ? stopPlay() : startPlay(cursor))} aria-label={play ? "Ferma" : "Ascolta"}>
                      {play ? <Square className="h-4 w-4" /> : <Play className="h-4 w-4" />} {play ? "Ferma" : "Ascolta"}
                    </button>
                    <input
                      type="range"
                      min={0}
                      max={Math.max(1, total)}
                      step={0.1}
                      value={Math.min(pos, total)}
                      onChange={(e) => {
                        const t = Number(e.target.value);
                        if (!play) return setCursor(t);
                        // while playing: move the thumb now, restart from there once the drag settles
                        setPlay({ ...play, t });
                        clearTimeout(seekTimer.current);
                        seekTimer.current = window.setTimeout(() => startPlay(t), 200);
                      }}
                      className="flex-1 accent-violet-600"
                      aria-label="Posizione"
                    />
                    <span className="w-24 text-right tabular-nums text-zinc-500">
                      {fmtDuration(pos)} / {fmtDuration(total)}
                    </span>
                    <button className="btn" onClick={exportAudio} disabled={exporting}>
                      {exporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />} Esporta…
                    </button>
                  </div>
                  {rendered.warnings.length > 0 && (
                    <div className="text-xs text-amber-700 dark:text-amber-300">
                      <button className="hover:underline" onClick={() => setShowWarnings(!showWarnings)}>
                        <AlertTriangle className="mr-1 inline h-3.5 w-3.5" />
                        {rendered.warnings.length} avvisi di pronuncia {showWarnings ? "▴" : "▾"}
                      </button>
                      {showWarnings && (
                        <ul className="mt-1 max-h-40 list-disc overflow-auto pl-5">
                          {rendered.warnings.map((w, i) => (
                            <li key={i}>{w}</li>
                          ))}
                        </ul>
                      )}
                    </div>
                  )}
                </>
              )}

              {exported && (
                <div className="flex items-center gap-3 rounded-lg border border-emerald-300 bg-emerald-50 p-2.5 text-emerald-900 dark:border-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-200">
                  <CheckCircle2 className="h-5 w-5 shrink-0" />
                  <div className="min-w-0 flex-1 truncate" title={exported}>
                    Esportato: {exported}
                  </div>
                  <button className="btn" onClick={() => revealItemInDir(exported)}>
                    <FolderOpen className="h-4 w-4" /> Mostra nel Finder
                  </button>
                </div>
              )}
            </section>

            <section className="card space-y-3">
              <div className="flex items-center gap-2 font-semibold">
                <FileMusic className="h-4 w-4 text-violet-600" /> Progetto OpenUtau (.ustx)
              </div>
              <div className="flex flex-wrap items-center gap-3">
                <button
                  className="btn"
                  onClick={generateUstx}
                  disabled={busy || sungSelected.length === 0 || missingSinger.length > 0 || notOpenUtau.length > 0}
                >
                  {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <FileMusic className="h-4 w-4" />} Crea .ustx
                </button>
                <span className="text-zinc-500">
                  {notOpenUtau.length > 0
                    ? `${notOpenUtau.join(", ")}: il cantante non è installato in OpenUtau.`
                    : "Per ritoccare fonemi e curve in OpenUtau, con gli stessi cantanti."}
                </span>
              </div>
              {result && (
                <div className="flex items-center gap-3 rounded-lg border border-emerald-300 bg-emerald-50 p-2.5 text-emerald-900 dark:border-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-200">
                  <CheckCircle2 className="h-5 w-5 shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="font-medium">
                      Creato: {result.tracks} tracce, {result.notes} note
                    </div>
                    <div className="truncate text-xs" title={result.output}>
                      {result.output}
                    </div>
                  </div>
                  <button className="btn" onClick={() => revealItemInDir(result.output)}>
                    <FolderOpen className="h-4 w-4" /> Mostra nel Finder
                  </button>
                </div>
              )}
            </section>
          </>
        )}

        {error && (
          <section className="flex items-start gap-2 rounded-xl border border-red-300 bg-red-50 p-3 text-red-800 dark:border-red-800 dark:bg-red-950/40 dark:text-red-200">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <div className="flex-1">{error}</div>
            <button onClick={() => setError(null)}>
              <X className="h-4 w-4" />
            </button>
          </section>
        )}

        <footer className="pt-2 text-xs text-zinc-500">
          L'audio è generato sul Mac con il motore DiffSinger di Muse Sing (derivato da OpenUtau). TIGER DS (tigermeat) è gratuito solo per uso
          non commerciale, citando l'autore.
        </footer>
      </main>
    </>
  );
}
