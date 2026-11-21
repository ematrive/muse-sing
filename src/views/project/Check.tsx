import { Headphones, Loader2, Mic2, Play, Square, X } from "lucide-react";
import type { CheckDone, RollBar, RollPart, Verdict } from "../../lib/api";
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
  return { judged: judged.length, inTune: count("in_tune"), almost: count("almost"), off: count("off"), missed: count("missed"), mean, bars: bad.map(([b]) => b) };
}

function summaryText(s: Summary) {
  if (s.judged === 0) return "Nessuna nota da valutare in questo tratto.";
  const pct = Math.round((s.inTune / s.judged) * 100);
  const parts = [`${pct}% intonate`];
  if (s.mean !== null && Math.abs(s.mean) >= 12) parts.push(s.mean < 0 ? `tendenzialmente calante (${Math.round(s.mean)} cent)` : `tendenzialmente crescente (+${Math.round(s.mean)} cent)`);
  if (s.bars.length > 0) parts.push(`da riprendere: ${s.bars.length === 1 ? "battuta" : "battute"} ${s.bars.slice(0, 8).join(", ")}${s.bars.length > 8 ? "…" : ""}`);
  return parts.join(" · ");
}

/**
 * «Canta e controlla»: sing your part over the music (count-in, then the stretch chosen on the
 * ruler, or from the cursor to the end), then see how each note went.
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
}: {
  mine: string | null;
  ownVoice: OwnVoice;
  onOwnVoice: (v: OwnVoice) => void;
  /** null: idle; true: count-in; false: singing */
  checking: boolean | null;
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
}) {
  return (
    <section className="card space-y-3">
      <div className="flex items-center gap-2 font-semibold">
        <Mic2 className="h-4 w-4 text-violet-600" /> Canta e controlla
      </div>
      {!mine ? (
        <p className="text-zinc-500">Scegli «la mia voce» accanto a una delle voci qui sopra: poi canti la tua parte sopra le altre e vedi come è andata, nota per nota.</p>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-3">
            {checking === null ? (
              <button className="btn-primary" onClick={onStart} disabled={disabled}>
                <Mic2 className="h-4 w-4" /> Canta
              </button>
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
              <select className="field w-auto py-0.5" aria-label="La tua voce nella base" value={ownVoice} onChange={(e) => onOwnVoice(e.target.value as OwnVoice)} disabled={checking !== null}>
                <option value="muta">muta</option>
                <option value="piano">piano</option>
                <option value="normale">normale</option>
              </select>
            </label>
            {checking !== null && (
              <span className="flex items-center gap-2 text-xs text-zinc-500">
                {checking ? (
                  <>
                    <Loader2 className="h-3.5 w-3.5 animate-spin" /> conteggio…
                  </>
                ) : (
                  "canta!"
                )}
                <span className="h-1.5 w-24 overflow-hidden rounded bg-zinc-200 dark:bg-zinc-800" title="Livello del microfono">
                  <span className="block h-full bg-emerald-500" style={{ width: `${Math.min(100, Math.round(level * 140))}%` }} />
                </span>
              </span>
            )}
          </div>
          <p className="flex items-center gap-1.5 text-xs text-zinc-500">
            <Headphones className="h-3.5 w-3.5" /> Usa le cuffie: altrimenti il microfono sente anche le altre voci. Microfono e cuffie sono quelli scelti in «Registra un cantante».
          </p>
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
                <p className="text-xs text-amber-700 dark:text-amber-300">Non ti ho sentito: controlla il microfono in «Registra un cantante».</p>
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
