import { AlertTriangle, Loader2, Mic, Mic2, MicOff, Play, Square, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { audioApi, errorText, type CheckDone, type Devices, type RollBar, type RollPart, type Verdict } from "../../lib/api";
import { serial } from "../../lib/serial";
import { MicCheck, type MicState } from "../record/MicCheck";
import { useStored } from "../record/useStored";
import { barAt } from "./bars";

/** How your own part sounds in the music while you sing. */
export type OwnVoice = "muta" | "piano" | "normale";
export const OWN_VOICE_DB: Record<OwnVoice, number | null> = { muta: null, piano: -12, normale: 0 };

export const VERDICT_CLASS: Record<Verdict, string> = {
  in_tune: "fill-emerald-500",
  almost: "fill-amber-400",
  off: "fill-red-500",
  missed: "fill-transparent stroke-red-500",
  not_judged: "",
};

type Summary = { judged: number; inTune: number; almost: number; off: number; missed: number; mean: number | null; bars: string[] };

/** Counts, the average deviation of the sung notes and the measures that went worst. */
export function summarize(result: CheckDone, part: RollPart | undefined, bars: RollBar[]): Summary {
  const judged = result.notes.filter((n) => n.verdict !== "not_judged");
  const count = (v: Verdict) => judged.filter((n) => n.verdict === v).length;
  const cents = judged.flatMap((n) => (n.cents === null ? [] : [n.cents]));
  const mean = cents.length >= 3 ? cents.reduce((a, b) => a + b, 0) / cents.length : null;
  // measures (printed numbers) where at least half of the notes, and at least two, went wrong
  const tickOf = new Map((part?.notes ?? []).map((n) => [n.id, n.tick]));
  const perBar = new Map<string, { n: number; bad: number; order: number }>();
  for (const n of judged) {
    const tick = tickOf.get(n.id);
    const bar = tick === undefined ? undefined : barAt(bars, tick);
    if (!bar?.number) continue;
    const e = perBar.get(bar.number) ?? { n: 0, bad: 0, order: bar.tick };
    e.n += 1;
    if (n.verdict === "off" || n.verdict === "missed") e.bad += 1;
    perBar.set(bar.number, e);
  }
  const bad = [...perBar.entries()].filter(([, e]) => e.bad >= 2 && e.bad * 2 >= e.n).sort((a, b) => a[1].order - b[1].order);
  return {
    judged: judged.length,
    inTune: count("in_tune"),
    almost: count("almost"),
    off: count("off"),
    missed: count("missed"),
    mean,
    bars: bad.map(([b]) => b),
  };
}

function summaryText(s: Summary) {
  if (s.judged === 0) return "Nessuna nota da valutare in questo tratto.";
  const pct = Math.round((s.inTune / s.judged) * 100);
  const parts = [`${pct}% intonate`];
  if (s.mean !== null && Math.abs(s.mean) >= 12)
    parts.push(s.mean < 0 ? `tendenzialmente calante (${Math.round(s.mean)} cent)` : `tendenzialmente crescente (+${Math.round(s.mean)} cent)`);
  if (s.bars.length > 0)
    parts.push(`da riprendere: ${s.bars.length === 1 ? "battuta" : "battute"} ${s.bars.slice(0, 8).join(", ")}${s.bars.length > 8 ? "…" : ""}`);
  return parts.join(" · ");
}

/** Below this level for a while while singing, the Mac is probably not passing the microphone. */
const SILENT_LEVEL = 0.003; // about −50 dB
const SILENT_MS = 3000;

type MicOpen = { state: "off" } | MicState;

/**
 * «Canta e controlla»: sing your part over the music (count-in, then the stretch chosen on the
 * ruler, or from the cursor to the end), then see how each note went. The microphone is opened
 * here (the same devices as Registra) and closed when leaving.
 */
export function CheckPanel({
  mine,
  ownVoice,
  onOwnVoice,
  checking,
  level,
  result,
  summary,
  stretch,
  onStart,
  onStop,
  onReplay,
  onClear,
  disabled,
  active,
}: {
  mine: string | null;
  ownVoice: OwnVoice;
  onOwnVoice: (v: OwnVoice) => void;
  checking: "counting" | "singing" | "analyzing" | null;
  level: number;
  result: CheckDone | null;
  summary: Summary | null;
  /** what will be sung, already described ("dalla battuta 5 alla fine") */
  stretch: string;
  onStart: () => void;
  onStop: () => void;
  onReplay: () => void;
  onClear: () => void;
  disabled: boolean;
  /** the project view is in front: the microphone closes when it is not */
  active: boolean;
}) {
  const [inputPref, setInputPref] = useStored("rec.input", "");
  const [outputPref, setOutputPref] = useStored("rec.output", "");
  const [mic, setMic] = useState<MicOpen>({ state: "off" });
  const [devices, setDevices] = useState<Devices | null>(null);
  const micOn = mic.state !== "off";

  const openMic = useCallback(() => {
    setMic({ state: "opening" });
    serial(() => audioApi.open(inputPref || null, outputPref || null))
      .then((status) => setMic({ state: "ok", status }))
      .catch((e) => setMic({ state: "error", error: errorText(e) }));
    audioApi
      .devices()
      .then(setDevices)
      .catch(() => {});
  }, [inputPref, outputPref]);
  const closeMic = () => {
    setMic({ state: "off" });
    serial(() => audioApi.close()).catch(() => {});
  };
  // another microphone or headphones chosen: reopen with them
  const firstPrefs = useRef(true);
  useEffect(() => {
    if (firstPrefs.current) {
      firstPrefs.current = false;
      return;
    }
    if (micOn && checking === null) openMic();
  }, [inputPref, outputPref]); // eslint-disable-line react-hooks/exhaustive-deps
  // leaving Studia or the project: microphone off, Bluetooth headphones back to full quality
  const micOnRef = useRef(micOn);
  micOnRef.current = micOn;
  useEffect(() => {
    if (active) return;
    if (micOnRef.current) closeMic();
  }, [active]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(
    () => () => {
      if (micOnRef.current) serial(() => audioApi.close()).catch(() => {});
    },
    [],
  );

  // nothing from the microphone while singing (checked on a timer: a level stuck at 0 never changes)
  const [silent, setSilent] = useState(false);
  const levelRef = useRef(level);
  levelRef.current = level;
  useEffect(() => {
    if (checking === "counting") setSilent(false);
    if (checking !== "singing") return;
    let loudAt = performance.now();
    const t = window.setInterval(() => {
      if (levelRef.current > SILENT_LEVEL) {
        loudAt = performance.now();
        setSilent(false);
      } else if (performance.now() - loudAt > SILENT_MS) setSilent(true);
    }, 250);
    return () => clearInterval(t);
  }, [checking]);

  return (
    <section className="card space-y-3">
      <div className="flex items-center gap-2 font-semibold">
        <Mic2 className="h-4 w-4 text-violet-600" /> Canta e controlla
      </div>
      {!mine ? (
        <p className="text-zinc-500">Scegli qui sopra in «Io canto» la tua voce: poi canti la tua parte sopra le altre e vedi come è andata, nota per nota.</p>
      ) : (
        <>
          <ol className="list-decimal space-y-1 pl-5 text-zinc-600 dark:text-zinc-400">
            <li>
              Mettiti le <b>cuffie</b> e accendi il microfono: la barra deve muoversi quando parli.
            </li>
            <li>
              Nel <b>Mixer</b> qui sopra abbassa o spegni le voci che non vuoi sentire.
            </li>
            <li>
              Premi <b>Canta</b>: dopo la battuta di conteggio canta la tua parte.
            </li>
          </ol>

          {!micOn ? (
            <button className="btn" onClick={openMic} disabled={disabled}>
              <Mic className="h-4 w-4" /> Accendi il microfono
            </button>
          ) : (
            <div className="space-y-2">
              <MicCheck
                mic={mic as MicState}
                devices={devices}
                input={inputPref}
                output={outputPref}
                setInput={setInputPref}
                setOutput={setOutputPref}
                issue={null}
                retry={openMic}
                disabled={checking !== null}
                showHint={false}
              />
              {checking === null && (
                <button className="text-xs text-zinc-500 hover:underline" onClick={closeMic}>
                  <MicOff className="mr-1 inline h-3.5 w-3.5" />
                  spegni il microfono
                </button>
              )}
            </div>
          )}

          <div className="flex flex-wrap items-center gap-3">
            {checking === null ? (
              <button className="btn-primary" onClick={onStart} disabled={disabled || mic.state !== "ok"} title={mic.state !== "ok" ? "Accendi prima il microfono" : undefined}>
                <Mic2 className="h-4 w-4" /> Canta
              </button>
            ) : checking === "analyzing" ? (
              <span className="btn opacity-70">
                <Loader2 className="h-4 w-4 animate-spin" /> Analizzo…
              </span>
            ) : (
              <button className="btn" onClick={onStop}>
                <Square className="h-4 w-4" /> Ferma
              </button>
            )}
            <span className="text-zinc-500">
              {mine}, {stretch}
            </span>
            <label className="flex items-center gap-2 text-xs text-zinc-500">
              la tua voce nella base
              <select
                className="field w-auto py-0.5"
                aria-label="La tua voce nella base"
                value={ownVoice}
                onChange={(e) => onOwnVoice(e.target.value as OwnVoice)}
                disabled={checking !== null}
              >
                <option value="muta">spenta</option>
                <option value="piano">piano (guida)</option>
                <option value="normale">come nel mixer</option>
              </select>
            </label>
            {checking === "counting" && (
              <span className="flex items-center gap-2 text-xs text-zinc-500">
                <Loader2 className="h-3.5 w-3.5 animate-spin" /> conteggio…
              </span>
            )}
            {checking === "singing" && <span className="text-xs font-medium text-emerald-600">canta!</span>}
          </div>
          {silent && (
            <div className="flex items-start gap-2 rounded-md bg-amber-50 p-2 text-xs text-amber-900 dark:bg-amber-950/40 dark:text-amber-200">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
              <div>
                Non arriva suono dal microfono. Su macOS apri <b>Impostazioni di Sistema → Privacy e sicurezza → Microfono</b> e attiva Muse Sing (o il Terminale, se
                avvii l'app da lì), poi riaccendi il microfono. Oppure scegli un altro microfono qui sopra.
              </div>
            </div>
          )}
          {result && summary && (
            <div className="space-y-2 rounded-md border border-zinc-200 p-3 dark:border-zinc-800">
              <div className="flex flex-wrap items-center gap-3">
                <span className="font-medium">{summaryText(summary)}</span>
                <button className="btn ml-auto py-1" onClick={onReplay} disabled={checking !== null}>
                  <Play className="h-4 w-4" /> Riascolta
                </button>
                <button className="text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200" onClick={onClear} title="Togli i colori dal rullo">
                  <X className="h-4 w-4" />
                </button>
              </div>
              <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-zinc-500">
                <Legend cls="bg-emerald-500" text={`intonate ${summary.inTune}`} />
                <Legend cls="bg-amber-400" text={`quasi ${summary.almost}`} />
                <Legend cls="bg-red-500" text={`stonate ${summary.off}`} />
                <Legend cls="border border-red-500" text={`non cantate ${summary.missed}`} />
                <span>la linea azzurra è la tua voce</span>
              </div>
              {summary.judged > 0 && summary.missed === summary.judged && (
                <p className="text-xs text-amber-700 dark:text-amber-300">
                  Non ti ho sentito cantare: controlla che la barra del microfono si muova quando parli.
                </p>
              )}
            </div>
          )}
        </>
      )}
    </section>
  );
}

function Legend({ cls, text }: { cls: string; text: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span className={`inline-block h-2 w-3 rounded-sm ${cls}`} /> {text}
    </span>
  );
}
