import { Eraser, Loader2, Play, RotateCcw, Waves } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { barAt, BarGrid } from "./bars";
import { CURVE_STEP, errorText, singApi, type Curve, type PartEdits, type Roll, type TrackInfo, type WordInfo } from "../../lib/api";

type Tool = "word" | "pitch" | "dynamics" | "gender" | "velocity";
type CurveTool = Exclude<Tool, "word">;

/** Range, default and labels of each curve. */
const CURVES: Record<CurveTool, { min: number; max: number; unit: string; low: string; high: string; label: string; hint: string }> = {
  pitch: { min: -200, max: 200, unit: "cent", low: "più basso", high: "più alto", label: "Intonazione", hint: "100 cent = un semitono" },
  dynamics: { min: -24, max: 12, unit: "dB", low: "più piano", high: "più forte", label: "Dinamica", hint: "non serve rigenerare la voce" },
  gender: {
    min: -1,
    max: 1,
    unit: "",
    low: "più scuro",
    high: "più chiaro",
    label: "Timbro",
    hint: "sperimentale: con TIGER oltre ±0,3 la voce si rovina",
  },
  velocity: { min: -0.5, max: 0.5, unit: "", low: "consonanti lente", high: "consonanti veloci", label: "Consonanti", hint: "velocità delle consonanti" },
};

const NOTES_H = 150;
const BAND_H = 90;
const WORDS_H = 26;
const RULER = 16;

/** Tick ↔ second over the tempo map of the roll (ds/time.rs). */
function makeAxis(tempos: [number, number][]) {
  const t = [...tempos].filter((x) => x[1] > 0).sort((a, b) => a[0] - b[0]);
  if (!t.length || t[0][0] > 0) t.unshift([0, t[0]?.[1] ?? 120]);
  const segs: { tick: number; sec: number; spt: number }[] = [];
  for (const [tick, bpm] of t) {
    const spt = 60 / (bpm * 480);
    const last = segs[segs.length - 1];
    const sec = last ? last.sec + (tick - last.tick) * last.spt : 0;
    if (last && last.tick === tick) last.spt = spt;
    else segs.push({ tick, sec, spt });
  }
  const bySec = (s: number) => {
    for (let i = segs.length - 1; i > 0; i--) if (segs[i].sec <= s) return segs[i];
    return segs[0];
  };
  const byTick = (k: number) => {
    for (let i = segs.length - 1; i > 0; i--) if (segs[i].tick <= k) return segs[i];
    return segs[0];
  };
  return {
    toSec: (tick: number) => {
      const s = byTick(tick);
      return s.sec + (tick - s.tick) * s.spt;
    },
    toTick: (sec: number) => {
      const s = bySec(sec);
      return s.tick + (sec - s.sec) / s.spt;
    },
  };
}

const snap = (tick: number) => Math.round(tick / CURVE_STEP) * CURVE_STEP;

/** Writes `points` (tick → value) into a curve, dropping default values. */
function mergeCurve(curve: Curve | undefined, points: Map<number, number>): Curve {
  const m = new Map<number, number>(curve ?? []);
  for (const [k, v] of points) {
    if (Math.abs(v) < 1e-4) m.delete(k);
    else m.set(k, v);
  }
  return [...m.entries()].sort((a, b) => a[0] - b[0]);
}

function curveAt(curve: Map<number, number>, tick: number) {
  const a = Math.floor(tick / CURVE_STEP) * CURVE_STEP;
  const f = (tick - a) / CURVE_STEP;
  const va = curve.get(a) ?? 0;
  const vb = curve.get(a + CURVE_STEP) ?? 0;
  return va + (vb - va) * f;
}

function fmtVal(tool: CurveTool, v: number) {
  const c = CURVES[tool];
  const r = tool === "gender" || tool === "velocity" ? v.toFixed(2) : Math.round(v).toString();
  return `${v > 0 ? "+" : ""}${r}${c.unit ? ` ${c.unit}` : ""}`;
}

/**
 * Edits of one sung part: pronunciation of the words (phonemes and their timing) and the curves
 * (intonation, dynamics, timbre, consonant speed), drawn by hand over the notes. Edits are saved
 * in the project; «Applica e ascolta» renders again (only the phrases that changed) and plays.
 */
export function Editor({
  parts,
  roll,
  edits,
  initialPart,
  rendered,
  upToDate,
  rendering,
  cursor,
  playing,
  onChange,
  onSeek,
  onApply,
}: {
  parts: string[];
  roll: Roll;
  edits: Record<string, PartEdits>;
  initialPart: string | null;
  /** Changes at every completed render (the editor reads the phonemes and pitch again). */
  rendered: unknown;
  upToDate: boolean;
  rendering: boolean;
  cursor: number;
  playing: boolean;
  onChange: (part: string, e: PartEdits) => void;
  onSeek: (sec: number) => void;
  /** Render with the edits, then play from `fromSec`. */
  onApply: (fromSec: number) => void;
}) {
  const [part, setPart] = useState(() => (initialPart && parts.includes(initialPart) ? initialPart : parts[0]));
  const [tool, setTool] = useState<Tool>("word");
  const [info, setInfo] = useState<TrackInfo | null>(null);
  const [infoError, setInfoError] = useState<string | null>(null);
  const [zoom, setZoom] = useState(90);
  const [eraser, setEraser] = useState(false);
  const [vibrato, setVibrato] = useState(false);
  const [wordId, setWordId] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!parts.includes(part)) setPart(parts[0]);
  }, [parts, part]);

  useEffect(() => {
    let stale = false;
    setInfoError(null);
    singApi.trackInfo(part).then(
      (i) => !stale && setInfo(i),
      (e) => !stale && setInfoError(errorText(e)),
    );
    return () => {
      stale = true;
    };
  }, [part, rendered]);

  const axis = useMemo(() => makeAxis(roll.tempos), [roll.tempos]);
  const notes = useMemo(() => roll.parts.find((p) => p.name === part)?.notes ?? [], [roll, part]);
  const e: PartEdits = useMemo(() => edits[part] ?? {}, [edits, part]);
  const total = useMemo(() => Math.max(1, ...notes.map((n) => n.end), ...(info?.words.map((w) => w.end) ?? [])), [notes, info]);
  const width = total * zoom + 60;
  const x = (sec: number) => sec * zoom;
  const curveTool: CurveTool | null = tool === "word" ? null : tool;
  const height = RULER + NOTES_H + (curveTool ? BAND_H : 0) + WORDS_H;

  // pitch range of the lane: the notes plus a margin
  const [lo, hi] = useMemo(() => {
    const p = notes.map((n) => n.pitch);
    return p.length ? [Math.min(...p) - 3, Math.max(...p) + 3] : [57, 75];
  }, [notes]);
  const yPitch = (m: number) => RULER + 4 + (NOTES_H - 8) * (1 - (m - lo) / (hi - lo));
  const bandTop = RULER + NOTES_H;
  const yVal = (t: CurveTool, v: number) => {
    const c = CURVES[t];
    return bandTop + 6 + (BAND_H - 12) * (1 - (Math.min(c.max, Math.max(c.min, v)) - c.min) / (c.max - c.min));
  };
  const valAt = (t: CurveTool, y: number) => {
    const c = CURVES[t];
    const r = 1 - (y - bandTop - 6) / (BAND_H - 12);
    return Math.min(c.max, Math.max(c.min, c.min + r * (c.max - c.min)));
  };
  const wordsTop = bandTop + (curveTool ? BAND_H : 0);

  // keep the cursor in view while playing
  useEffect(() => {
    const el = scroller.current;
    if (!el || !playing) return;
    const cx = x(cursor);
    if (cx < el.scrollLeft + 40 || cx > el.scrollLeft + el.clientWidth - 80) el.scrollLeft = Math.max(0, cx - el.clientWidth * 0.25);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cursor, playing, zoom]);

  // --- drawing curves ------------------------------------------------------------------------------

  const [draft, setDraft] = useState<Map<number, number> | null>(null);
  const draftRef = useRef<Map<number, number> | null>(null);
  const stroke = useRef<{ tick: number; value: number } | null>(null);
  const base = useMemo(() => (curveTool ? new Map<number, number>(e[curveTool] ?? []) : null), [curveTool, e]);

  const pointAt = (ev: React.PointerEvent) => {
    const rect = (ev.currentTarget as SVGElement).ownerSVGElement!.getBoundingClientRect();
    return { sec: Math.max(0, (ev.clientX - rect.left) / zoom), y: ev.clientY - rect.top };
  };

  const paint = (ev: React.PointerEvent, first: boolean) => {
    if (!curveTool) return;
    const { sec, y } = pointAt(ev);
    const tick = snap(axis.toTick(sec));
    const erase = eraser || ev.altKey || ev.button === 2;
    const value = erase ? 0 : valAt(curveTool, y);
    const prev = first ? null : stroke.current;
    const next = first || !draftRef.current ? new Map<number, number>() : draftRef.current;
    const [a, b] = prev ? [Math.min(prev.tick, tick), Math.max(prev.tick, tick)] : [tick, tick];
    for (let k = a; k <= b; k += CURVE_STEP) {
      const f = prev && b > a ? (k - prev.tick) / (tick - prev.tick) : 1;
      next.set(k, erase ? 0 : prev ? prev.value + (value - prev.value) * f : value);
    }
    stroke.current = { tick, value };
    draftRef.current = next;
    setDraft(new Map(next));
  };

  const commit = () => {
    const d = draftRef.current;
    draftRef.current = null;
    stroke.current = null;
    setDraft(null);
    if (curveTool && d) onChange(part, { ...e, [curveTool]: mergeCurve(e[curveTool], d) });
  };

  const addVibrato = (n: (typeof notes)[number]) => {
    const pts = new Map<number, number>();
    const len = n.end - n.start;
    const from = n.start + len * 0.3;
    for (let k = snap(axis.toTick(from)); k <= n.end_tick; k += CURVE_STEP) {
      const t = axis.toSec(k) - from;
      const fade = Math.min(1, t / Math.max(0.15, (n.end - from) * 0.4)) * Math.min(1, (n.end - axis.toSec(k)) / 0.08);
      pts.set(k, 35 * fade * Math.sin(2 * Math.PI * 5.5 * t));
    }
    onChange(part, { ...e, pitch: mergeCurve(e.pitch, pts) });
  };
  const vibratoRef = useRef(addVibrato);
  vibratoRef.current = addVibrato;

  const clearCurve = () => curveTool && onChange(part, { ...e, [curveTool]: [] });

  // --- layers (memoized: the cursor moves many times a second while playing) ------------------------

  const barOf = (id: string) => {
    const n = notes.find((n) => n.id === id);
    return n ? barAt(roll.bars ?? [], n.tick)?.number : undefined;
  };

  const grid = useMemo(
    () =>
      roll.bars?.length ? (
        <BarGrid bars={roll.bars} zoom={zoom} ruler={RULER} bottom={RULER + NOTES_H + BAND_H + WORDS_H} />
      ) : (
        Array.from({ length: Math.ceil(total / 5) + 1 }, (_, i) => i * 5).map((t) => (
          <g key={t} pointerEvents="none">
            <line x1={t * zoom} x2={t * zoom} y1={RULER - 5} y2={RULER + NOTES_H + BAND_H + WORDS_H} className="stroke-zinc-200 dark:stroke-zinc-800" />
            <text x={t * zoom + 3} y={11} className="fill-zinc-500 text-[10px]">
              {Math.floor(t / 60)}:{String(t % 60).padStart(2, "0")}
            </text>
          </g>
        ))
      ),
    [total, zoom, roll.bars],
  );

  const noteLayer = useMemo(
    () =>
      notes.map((n) => (
        <rect
          key={n.id}
          x={n.start * zoom}
          y={yPitch(n.pitch) - 3}
          width={Math.max(2, (n.end - n.start) * zoom - 1)}
          height={6}
          rx={2}
          className={vibrato ? "cursor-pointer fill-violet-300 hover:fill-violet-500 dark:fill-violet-800" : "fill-zinc-300 dark:fill-zinc-700"}
          pointerEvents={vibrato ? "all" : "none"}
          onPointerDown={() => vibratoRef.current(n)}
        >
          {vibrato && <title>Aggiungi vibrato</title>}
        </rect>
      )),
    [notes, zoom, vibrato, lo, hi], // eslint-disable-line react-hooks/exhaustive-deps
  );

  // the pitch actually sung, at most one point per pixel
  const pitchLayer = useMemo(
    () =>
      info?.pitch.map((seg, i) => {
        const every = Math.max(1, Math.floor(1 / (seg.step * zoom)));
        const pts: string[] = [];
        for (let k = 0; k < seg.midi.length; k += every) pts.push(`${((seg.start + k * seg.step) * zoom).toFixed(1)},${yPitch(seg.midi[k]).toFixed(1)}`);
        return <polyline key={i} points={pts.join(" ")} fill="none" pointerEvents="none" className="stroke-violet-600 dark:stroke-violet-400" strokeWidth={1.3} />;
      }),
    [info, zoom, lo, hi], // eslint-disable-line react-hooks/exhaustive-deps
  );

  // while drawing the intonation: the sung pitch with the new stroke, over the stroke only
  const pitchPreview = useMemo(() => {
    if (tool !== "pitch" || !draft || !info || !base || draft.size === 0) return null;
    const keys = [...draft.keys()];
    const [a, b] = [axis.toSec(Math.min(...keys)), axis.toSec(Math.max(...keys))];
    return info.pitch
      .filter((seg) => seg.start <= b && seg.start + seg.step * seg.midi.length >= a)
      .map((seg, i) => {
        const pts: string[] = [];
        for (let k = Math.max(0, Math.floor((a - seg.start) / seg.step)); k < seg.midi.length; k++) {
          const sec = seg.start + k * seg.step;
          if (sec > b) break;
          const tick = axis.toTick(sec);
          const shift = (curveAt(draft, tick) - curveAt(base, tick)) / 100;
          pts.push(`${x(sec).toFixed(1)},${yPitch(seg.midi[k] + shift).toFixed(1)}`);
        }
        return <polyline key={i} points={pts.join(" ")} fill="none" pointerEvents="none" className="stroke-amber-500" strokeWidth={1.3} />;
      });
  }, [tool, draft, info, base, axis, zoom, lo, hi]); // eslint-disable-line react-hooks/exhaustive-deps

  /** Runs of drawn points as polylines, back to the default line next to a gap. */
  const runs = (m: Map<number, number>, t: CurveTool, cls: string) => {
    const keys = [...m.entries()].filter(([, v]) => Math.abs(v) > 1e-4).sort((p, q) => p[0] - q[0]);
    const out: [number, number][][] = [];
    for (const p of keys) {
      const last = out[out.length - 1];
      if (last && p[0] - last[last.length - 1][0] <= CURVE_STEP) last.push(p);
      else out.push([p]);
    }
    return out.map((r, i) => {
      const ends = [[r[0][0] - CURVE_STEP, 0], ...r, [r[r.length - 1][0] + CURVE_STEP, 0]];
      const pts = ends.map(([k, v]) => `${x(axis.toSec(k)).toFixed(1)},${yVal(t, v).toFixed(1)}`).join(" ");
      return <polyline key={i} points={pts} fill="none" pointerEvents="none" className={cls} strokeWidth={1.6} />;
    });
  };
  const curveLayer = useMemo(
    () => (curveTool && base ? runs(base, curveTool, "stroke-amber-500") : null),
    [curveTool, base, axis, zoom], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const draftLayer = useMemo(
    () => (curveTool && draft ? runs(draft, curveTool, eraser ? "stroke-rose-500" : "stroke-amber-600") : null),
    [curveTool, draft, axis, zoom, eraser], // eslint-disable-line react-hooks/exhaustive-deps
  );

  const wordLayer = useMemo(
    () =>
      info?.words.map((w) => {
        const edited = !!e.notes?.[w.id] || !!e.words?.[w.word];
        const sel = w.id === wordId;
        const from = w.phones[0]?.start ?? w.start;
        return (
          <g
            key={w.id}
            className={tool === "word" ? "cursor-pointer" : ""}
            onPointerDown={() => {
              if (tool !== "word") return;
              setWordId(w.id);
              onSeek(Math.max(0, from));
            }}
          >
            <rect
              x={from * zoom}
              y={wordsTop + 2}
              width={Math.max(4, (w.end - from) * zoom - 1)}
              height={WORDS_H - 4}
              rx={3}
              className={sel ? "fill-violet-600/25" : edited ? "fill-amber-400/25" : "fill-zinc-100 dark:fill-zinc-800/60"}
            />
            {tool === "word" &&
              w.phones
                .slice(1)
                .map((p, k) => (
                  <line key={k} x1={p.start * zoom} x2={p.start * zoom} y1={wordsTop + 4} y2={wordsTop + WORDS_H - 4} className="stroke-zinc-400/60" pointerEvents="none" />
                ))}
            <text x={w.start * zoom + 2} y={wordsTop + 17} className={`text-[11px] ${sel ? "fill-violet-800 dark:fill-violet-200" : "fill-zinc-600 dark:fill-zinc-300"}`}>
              {w.word}
            </text>
          </g>
        );
      }),
    [info, e, wordId, tool, zoom, wordsTop], // eslint-disable-line react-hooks/exhaustive-deps
  );

  // --- words ---------------------------------------------------------------------------------------

  const word = info?.words.find((w) => w.id === wordId) ?? null;
  const editsCount =
    Object.keys(e.notes ?? {}).length + Object.keys(e.words ?? {}).length + (["pitch", "dynamics", "gender", "velocity"] as const).filter((k) => e[k]?.length).length;
  const tools: [Tool, string][] = [
    ["word", "Pronuncia"],
    ["pitch", "Intonazione"],
    ["dynamics", "Dinamica"],
    ...(info?.gender !== false ? ([["gender", "Timbro"]] as [Tool, string][]) : []),
    ...(info?.velocity !== false ? ([["velocity", "Consonanti"]] as [Tool, string][]) : []),
  ];

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-3">
        <span className="font-semibold">Ritocchi</span>
        <select className="field w-40" value={part} onChange={(ev) => setPart(ev.target.value)} aria-label="Voce da ritoccare">
          {parts.map((p) => (
            <option key={p}>{p}</option>
          ))}
        </select>
        <div className="seg">
          {tools.map(([t, label]) => (
            <button
              key={t}
              className={`px-3 ${tool === t ? "bg-violet-600 text-white" : "hover:bg-zinc-100 dark:hover:bg-zinc-800"}`}
              onClick={() => {
                setTool(t);
                setVibrato(false);
              }}
            >
              {label}
            </button>
          ))}
        </div>
        {editsCount > 0 && <span className="text-xs text-zinc-500">{editsCount} ritocchi su questa voce</span>}
        <span className="ml-auto text-xs text-zinc-500">Zoom</span>
        <input type="range" min={30} max={260} value={zoom} onChange={(ev) => setZoom(Number(ev.target.value))} className="w-24 accent-violet-600" aria-label="Zoom dei ritocchi" />
      </div>

      <div className="text-xs text-zinc-500">
        {curveTool ? (
          <>
            {CURVES[curveTool].label}: disegna nella fascia sotto le note ({CURVES[curveTool].high} in alto, {CURVES[curveTool].low} in basso;{" "}
            {CURVES[curveTool].hint}). Con ⌥ o la gomma cancelli.
          </>
        ) : info ? (
          "Clicca una parola sotto le note per cambiarne i fonemi o i tempi."
        ) : (
          "Genera l'audio per vedere i fonemi e l'intonazione cantata."
        )}
        {infoError && <span className="text-red-600"> {infoError}</span>}
      </div>

      <div ref={scroller} className="overflow-x-auto rounded-lg border border-zinc-200 dark:border-zinc-800">
        <svg width={width} height={height} className="block select-none" onContextMenu={(ev) => curveTool && ev.preventDefault()}>
          {/* ruler and lane background: click = move the cursor */}
          <rect x={0} y={0} width={width} height={RULER} className="fill-zinc-100 dark:fill-zinc-900" onPointerDown={(ev) => onSeek(pointAt(ev).sec)} />
          <rect x={0} y={RULER} width={width} height={NOTES_H} className="fill-transparent" onPointerDown={(ev) => !vibrato && onSeek(pointAt(ev).sec)} />
          {grid}
          {noteLayer}
          {pitchLayer}
          {pitchPreview}

          {curveTool && (
            <g>
              <rect
                x={0}
                y={bandTop}
                width={width}
                height={BAND_H}
                className={`cursor-crosshair ${eraser ? "fill-rose-500/5" : "fill-violet-500/5"}`}
                onPointerDown={(ev) => {
                  (ev.target as Element).setPointerCapture(ev.pointerId);
                  paint(ev, true);
                }}
                onPointerMove={(ev) => stroke.current && paint(ev, false)}
                onPointerUp={commit}
              />
              <line x1={0} x2={width} y1={yVal(curveTool, 0)} y2={yVal(curveTool, 0)} pointerEvents="none" className="stroke-zinc-300 dark:stroke-zinc-700" strokeDasharray="4 4" />
              <text x={4} y={bandTop + 12} pointerEvents="none" className="fill-zinc-400 text-[10px]">
                {fmtVal(curveTool, CURVES[curveTool].max)}
              </text>
              <text x={4} y={bandTop + BAND_H - 4} pointerEvents="none" className="fill-zinc-400 text-[10px]">
                {fmtVal(curveTool, CURVES[curveTool].min)}
              </text>
              {curveLayer}
              {draftLayer}
            </g>
          )}

          {wordLayer}
          <line x1={x(cursor)} x2={x(cursor)} y1={0} y2={height} pointerEvents="none" className="stroke-red-500" strokeWidth={1.5} />
        </svg>
      </div>

      {curveTool && (
        <div className="flex flex-wrap items-center gap-2">
          <button className={`btn ${eraser ? "ring-2 ring-rose-400" : ""}`} onClick={() => setEraser(!eraser)} title="Cancella dove passi">
            <Eraser className="h-4 w-4" /> Gomma
          </button>
          {curveTool === "pitch" && (
            <button className={`btn ${vibrato ? "ring-2 ring-violet-400" : ""}`} onClick={() => setVibrato(!vibrato)} title="Poi clicca le note a cui aggiungerlo">
              <Waves className="h-4 w-4" /> Vibrato sulle note
            </button>
          )}
          <button className="btn" onClick={clearCurve} disabled={!e[curveTool]?.length}>
            <RotateCcw className="h-4 w-4" /> Togli tutta la curva
          </button>
          <ApplyButton upToDate={upToDate} rendering={rendering} onClick={() => onApply(Math.max(0, cursor - 1))} />
        </div>
      )}

      {tool === "word" && word && info && (
        <WordPanel
          key={word.id + JSON.stringify(e.notes?.[word.id] ?? null) + (e.words?.[word.word] ?? "")}
          word={word}
          bar={barOf(word.id)}
          info={info}
          edits={e}
          onChange={(next) => onChange(part, next)}
          upToDate={upToDate}
          rendering={rendering}
          onApply={() => onApply(Math.max(0, (word.phones[0]?.start ?? word.start) - 1))}
        />
      )}
    </div>
  );
}

function ApplyButton({ upToDate, rendering, onClick }: { upToDate: boolean; rendering: boolean; onClick: () => void }) {
  return (
    <button className="btn-primary" onClick={onClick} disabled={rendering}>
      {rendering ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />} {upToDate ? "Ascolta" : "Applica e ascolta"}
    </button>
  );
}

/** Phonemes and timing of one word. */
function WordPanel({
  word,
  bar,
  info,
  edits,
  onChange,
  upToDate,
  rendering,
  onApply,
}: {
  word: WordInfo;
  /** printed number of the measure where the word starts */
  bar: string | null | undefined;
  info: TrackInfo;
  edits: PartEdits;
  onChange: (e: PartEdits) => void;
  upToDate: boolean;
  rendering: boolean;
  onApply: () => void;
}) {
  const own = edits.notes?.[word.id];
  const table = edits.words?.[word.word];
  const shownPhones = word.phones.map((p) => p.symbol).join(" ");
  const [text, setText] = useState(own?.phonemes ?? table ?? shownPhones);
  const [everywhere, setEverywhere] = useState(!own?.phonemes && !!table);
  const known = new Set(info.phonemes.map((p) => p[0]));
  const unknown = text.split(/\s+/).filter((s) => s && !known.has(s));
  const changed = text.trim() !== shownPhones;

  const setNote = (patch: Partial<{ phonemes: string | null; offsets: number[] }>) => {
    const notes = { ...(edits.notes ?? {}) };
    const next = { ...(notes[word.id] ?? {}), ...patch, word: word.word };
    if (!next.phonemes) delete next.phonemes;
    if (!next.offsets?.some((o) => o !== 0)) delete next.offsets;
    if (next.phonemes || next.offsets) notes[word.id] = next;
    else delete notes[word.id];
    return notes;
  };

  /** Saves the phonemes (here or for every time the word is sung). */
  const savePhonemes = () => {
    const ph = text.trim().replace(/\s+/g, " ");
    if (!changed && !own?.phonemes && !table) return edits;
    if (everywhere) {
      const words = { ...(edits.words ?? {}), [word.word]: ph };
      return { ...edits, words, notes: setNote({ phonemes: null, offsets: [] }) };
    }
    return { ...edits, notes: setNote({ phonemes: ph, offsets: [] }) };
  };

  // timing: drag a phoneme's start
  const span: [number, number] = [Math.min(word.start, ...word.phones.map((p) => p.base)) - 0.12, word.end + 0.05];
  const W = 520;
  const px = (s: number) => ((s - span[0]) / (span[1] - span[0])) * W;
  const [offsets, setOffsets] = useState<number[]>(() => word.phones.map((p) => Math.round((p.start - p.base) * 1000)));
  const drag = useRef<number | null>(null);
  const starts = word.phones.map((p, i) => p.base + offsets[i] / 1000);
  const timingChanged = offsets.some((o, i) => o !== Math.round((word.phones[i].start - word.phones[i].base) * 1000));

  const onMove = (ev: React.PointerEvent<SVGSVGElement>) => {
    const i = drag.current;
    if (i === null) return;
    const rect = ev.currentTarget.getBoundingClientRect();
    const sec = span[0] + ((ev.clientX - rect.left) / W) * (span[1] - span[0]);
    const lo = i > 0 ? starts[i - 1] + 0.012 : span[0];
    const hi = i + 1 < starts.length ? starts[i + 1] - 0.012 : word.end - 0.012;
    const s = Math.min(hi, Math.max(lo, sec));
    setOffsets(offsets.map((o, k) => (k === i ? Math.round((s - word.phones[i].base) * 1000) : o)));
  };

  const apply = () => {
    let next = savePhonemes();
    if (!changed && timingChanged) {
      const notes = { ...(next.notes ?? {}) };
      const cur = { ...(notes[word.id] ?? {}), word: word.word };
      if (offsets.some((o) => o !== 0)) cur.offsets = offsets;
      else delete cur.offsets;
      if (cur.phonemes || cur.offsets) notes[word.id] = cur;
      else delete notes[word.id];
      next = { ...next, notes };
    }
    onChange(next);
    onApply();
  };

  const reset = () => {
    const notes = { ...(edits.notes ?? {}) };
    delete notes[word.id];
    onChange({ ...edits, notes });
  };

  const removeFromTable = () => {
    const words = { ...(edits.words ?? {}) };
    delete words[word.word];
    onChange({ ...edits, words });
  };

  return (
    <div className="space-y-3 rounded-lg border border-violet-200 bg-violet-50/40 p-3 dark:border-violet-900 dark:bg-violet-950/20">
      <div className="flex flex-wrap items-baseline gap-2">
        <span className="text-base font-semibold">«{word.word}»</span>
        <span className="text-xs text-zinc-500">
          battuta {bar ?? word.id.split(".")[0]}
          {word.id.split(".")[1] !== "1" ? ` (${word.id.split(".")[1]}ª volta)` : ""} · cantata: {shownPhones}
        </span>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <label className="text-xs text-zinc-500" htmlFor="phonemes">
          Fonemi
        </label>
        <input
          id="phonemes"
          className="field w-64 font-mono"
          value={text}
          onChange={(ev) => setText(ev.target.value)}
          spellCheck={false}
          aria-label="Fonemi della parola"
        />
        <label className="flex items-center gap-1 text-xs">
          <input type="radio" checked={!everywhere} onChange={() => setEverywhere(false)} className="accent-violet-600" /> solo qui
        </label>
        <label className="flex items-center gap-1 text-xs">
          <input type="radio" checked={everywhere} onChange={() => setEverywhere(true)} className="accent-violet-600" /> ogni volta che c'è «{word.word}»
        </label>
      </div>
      {unknown.length > 0 && <div className="text-xs text-amber-700 dark:text-amber-300">Il cantante non conosce: {unknown.join(", ")} (verranno saltati).</div>}
      <div className="flex flex-wrap gap-1">
        {info.phonemes.map(([s, vowel]) => (
          <button
            key={s}
            className={`rounded px-1.5 py-0.5 font-mono text-xs ${vowel ? "bg-violet-100 text-violet-800 dark:bg-violet-900/50 dark:text-violet-200" : "bg-zinc-100 text-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"}`}
            onClick={() => setText((t) => `${t.trim()} ${s}`.trim())}
            title={vowel ? "vocale" : "consonante"}
          >
            {s}
          </button>
        ))}
      </div>

      <div>
        <div className="mb-1 text-xs text-zinc-500">
          Tempi: trascina l'inizio di un fonema {changed && "(prima applica i fonemi nuovi)"}
        </div>
        <svg
          width={W}
          height={34}
          className={`block select-none rounded bg-white dark:bg-zinc-900 ${changed ? "opacity-40" : ""}`}
          onPointerMove={onMove}
          onPointerUp={() => (drag.current = null)}
          onPointerLeave={() => (drag.current = null)}
        >
          <line x1={px(word.start)} x2={px(word.start)} y1={0} y2={34} className="stroke-zinc-400" strokeDasharray="3 3" />
          {word.phones.map((p, i) => {
            const a = px(starts[i]);
            const b = px(i + 1 < starts.length ? starts[i + 1] : word.end);
            return (
              <g key={i}>
                <rect x={a} y={6} width={Math.max(1, b - a - 1)} height={22} rx={3} className={offsets[i] ? "fill-amber-300/60" : "fill-violet-200 dark:fill-violet-900"} />
                <text x={a + 3} y={21} className="fill-zinc-800 font-mono text-[11px] dark:fill-zinc-100" pointerEvents="none">
                  {p.symbol}
                </text>
                <rect
                  x={a - 3}
                  y={2}
                  width={6}
                  height={30}
                  className={changed ? "" : "cursor-ew-resize fill-transparent hover:fill-violet-500/40"}
                  onPointerDown={(ev) => {
                    if (changed) return;
                    (ev.currentTarget.ownerSVGElement as SVGSVGElement).setPointerCapture(ev.pointerId);
                    drag.current = i;
                  }}
                >
                  <title>{offsets[i] ? `${offsets[i] > 0 ? "+" : ""}${offsets[i]} ms` : "inizio di " + p.symbol}</title>
                </rect>
              </g>
            );
          })}
        </svg>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <ApplyButton upToDate={upToDate && !changed && !timingChanged} rendering={rendering} onClick={apply} />
        {own && (
          <button className="btn" onClick={reset}>
            <RotateCcw className="h-4 w-4" /> Ripristina questa parola
          </button>
        )}
        {table && (
          <span className="text-xs text-zinc-500">
            In tutto il brano «{word.word}» = <span className="font-mono">{table}</span>{" "}
            <button className="btn ml-1" onClick={removeFromTable}>
              Togli
            </button>
          </span>
        )}
      </div>
    </div>
  );
}
