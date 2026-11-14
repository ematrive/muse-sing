import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AlertTriangle, CheckCircle2, FileMusic, FolderOpen, Loader2, Mic2, RefreshCw, Upload, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorText, LANGS, type Analysis, type GenerateResult, type Lang, type SingersList } from "../lib/api";

function fmtDuration(sec: number) {
  const m = Math.floor(sec / 60);
  const s = Math.round(sec % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

export default function Convert({ active }: { active: boolean }) {
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [lang, setLang] = useState<Lang>("it");
  const [selectedParts, setSelectedParts] = useState<string[]>([]);
  const [singers, setSingers] = useState<SingersList | null>(null);
  const [singerByLang, setSingerByLang] = useState<Partial<Record<Lang, string>>>({});
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<GenerateResult | null>(null);

  const loadSingers = useCallback(async () => {
    try {
      const s = await api.listSingers();
      setSingers(s);
      setSingerByLang((cur) => ({ ...s.selected, ...Object.fromEntries(Object.entries(cur).filter(([, v]) => s.singers.some((x) => x.id === v))) }));
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  const loadFile = useCallback(async (path: string) => {
    setError(null);
    setResult(null);
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

  const pickFile = async () => {
    const f = await open({ multiple: false, filters: [{ name: "MuseScore", extensions: ["mscz"] }] });
    if (typeof f === "string") loadFile(f);
  };

  const singer = singerByLang[lang] ?? "";
  const singerInfo = singers?.singers.find((s) => s.id === singer);

  const generate = async () => {
    if (!analysis) return;
    setError(null);
    setResult(null);
    const output = await save({ defaultPath: analysis.default_output, filters: [{ name: "Progetto OpenUtau", extensions: ["ustx"] }] });
    if (!output) return;
    setBusy(true);
    try {
      setResult(await api.generate({ path: analysis.path, output, lang, singer, parts: selectedParts }));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const togglePart = (name: string) =>
    setSelectedParts((cur) => (cur.includes(name) ? cur.filter((n) => n !== name) : [...cur, name]));

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
              <button className="btn" onClick={pickFile} disabled={busy}>
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
                <select className="field" value={lang} onChange={(e) => setLang(e.target.value as Lang)}>
                  {Object.entries(LANGS).map(([k, v]) => (
                    <option key={k} value={k}>
                      {v}
                      {k === analysis.lang ? " (riconosciuta)" : ""}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="label flex items-center justify-between">
                  <span>Cantante (voicebank DiffSinger di OpenUtau)</span>
                  <button className="text-violet-600 hover:underline" onClick={loadSingers} title="Rileggi i cantanti installati">
                    <RefreshCw className="inline h-3 w-3" /> aggiorna
                  </button>
                </label>
                {singers && singers.singers.length > 0 ? (
                  <select className="field" value={singer} onChange={(e) => setSingerByLang({ ...singerByLang, [lang]: e.target.value })}>
                    <option value="">— scegli —</option>
                    {singers.singers.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name}
                        {s.singer_type !== "diffsinger" ? " (non DiffSinger)" : s.langs.includes(lang) ? "" : ` (senza dizionario ${lang})`}
                      </option>
                    ))}
                  </select>
                ) : (
                  <div className="rounded-md border border-zinc-300 p-2 text-zinc-500 dark:border-zinc-700">
                    Nessun cantante trovato in {singers?.folders.join(", ") || "OpenUtau"}. Installa un voicebank in OpenUtau e premi
                    «aggiorna».
                  </div>
                )}
                {singerInfo && singerInfo.singer_type !== "diffsinger" && (
                  <div className="mt-1 text-amber-600">Il file usa i phonemizer DiffSinger: scegli un voicebank DiffSinger.</div>
                )}
              </div>
            </section>

            <section className="card">
              <div className="label">Voci da cantare</div>
              <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
                {analysis.parts.map((p) => (
                  <label key={p.name} className={`flex items-start gap-3 py-2 ${p.sung ? "cursor-pointer" : "opacity-50"}`}>
                    <input
                      type="checkbox"
                      className="mt-0.5 accent-violet-600"
                      disabled={!p.sung}
                      checked={selectedParts.includes(p.name)}
                      onChange={() => togglePart(p.name)}
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
                  </label>
                ))}
              </div>
            </section>

            <div className="flex items-center gap-3">
              <button className="btn-primary" onClick={generate} disabled={busy || !singer || selectedParts.length === 0}>
                {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Mic2 className="h-4 w-4" />} Crea .ustx
              </button>
              {!singer && <span className="text-zinc-500">Scegli prima il cantante.</span>}
            </div>

            {result && (
              <section className="flex items-center gap-3 rounded-xl border border-emerald-300 bg-emerald-50 p-3 text-emerald-900 dark:border-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-200">
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
              </section>
            )}
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
          Poi apri il file .ustx in OpenUtau (File → Apri) e premi Play. Le voci: Gahata Meiji (LUNAI Project) e TIGER DS (tigermeat) sono
          gratuite solo per uso non commerciale, citando l’autore.
        </footer>
      </main>
    </>
  );
}
