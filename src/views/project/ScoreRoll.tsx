import { useEffect, useMemo, useRef, useState } from "react";
import type { RollBar, RollPart, Verdict } from "../../lib/api";
import { BarGrid } from "./bars";
import { VERDICT_CLASS } from "./Check";

/** The result of «Canta e controlla» drawn on a part: coloured notes and the sung pitch. */
export type RollCheck = { part: string; verdicts: Record<string, Verdict>; line: [number, number][] };

const RULER = 20;
const LANE = 58;
const LABEL = 92;

function fmt(sec: number) {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

/**
 * Every part of the score as a lane of notes with their syllables, the cursor of the playback
 * and the loop range. Click (lanes or ruler) to move the cursor; drag on the ruler to choose a
 * stretch to repeat.
 */
export function ScoreRoll({
  parts,
  bars,
  cursor,
  playing,
  highlight,
  loop,
  onSeek,
  onLoop,
  check,
}: {
  parts: RollPart[];
  /** measures as played; without them the grid is in seconds */
  bars: RollBar[];
  cursor: number;
  playing: boolean;
  highlight: string | null;
  loop: [number, number] | null;
  onSeek: (sec: number) => void;
  onLoop: (range: [number, number] | null) => void;
  check?: RollCheck | null;
}) {
  const [zoom, setZoom] = useState(70); // px per second
  const scroller = useRef<HTMLDivElement>(null);
  const drag = useRef<{ from: number; moved: boolean } | null>(null);
  const [selecting, setSelecting] = useState<[number, number] | null>(null);

  const total = useMemo(() => Math.max(1, ...parts.flatMap((p) => p.notes.map((n) => n.end))), [parts]);
  const width = total * zoom + 40;
  const height = RULER + parts.length * LANE;
  const x = (sec: number) => sec * zoom;

  // keep the cursor in view while playing
  useEffect(() => {
    const el = scroller.current;
    if (!el || !playing) return;
    const cx = x(cursor);
    if (cx < el.scrollLeft + 40 || cx > el.scrollLeft + el.clientWidth - 80) el.scrollLeft = Math.max(0, cx - el.clientWidth * 0.25);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cursor, playing, zoom]);

  const secAt = (e: React.PointerEvent) => {
    const rect = (e.currentTarget as SVGElement).getBoundingClientRect();
    return Math.max(0, Math.min(total, (e.clientX - rect.left) / zoom));
  };

  const ticks = useMemo(() => {
    const step = zoom >= 60 ? 5 : zoom >= 25 ? 10 : 30;
    const out: number[] = [];
    for (let t = 0; t <= total; t += step) out.push(t);
    return out;
  }, [total, zoom]);

  const shownLoop = selecting ?? loop;

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-3 text-xs text-zinc-500">
        <span>Clic per spostarti · trascina sul righello per ripetere un tratto</span>
        <span className="ml-auto">Zoom</span>
        <input
          type="range"
          min={15}
          max={160}
          value={zoom}
          onChange={(e) => setZoom(Number(e.target.value))}
          className="w-28 accent-violet-600"
          aria-label="Zoom del rullo"
        />
      </div>
      <div className="flex overflow-hidden rounded-lg border border-zinc-200 dark:border-zinc-800">
        <div className="shrink-0 border-r border-zinc-200 bg-zinc-50 text-xs dark:border-zinc-800 dark:bg-zinc-900" style={{ width: LABEL }}>
          <div style={{ height: RULER }} />
          {parts.map((p) => (
            <div
              key={p.name}
              className={`flex items-center truncate border-t border-zinc-200 px-2 dark:border-zinc-800 ${
                p.name === highlight ? "font-semibold text-violet-700 dark:text-violet-300" : p.sung ? "" : "text-zinc-400"
              }`}
              style={{ height: LANE }}
              title={p.name}
            >
              {p.name}
            </div>
          ))}
        </div>
        <div ref={scroller} className="min-w-0 flex-1 overflow-x-auto">
          <svg width={width} height={height} className="block select-none">
            {/* ruler: click = seek, drag = loop range */}
            <rect
              x={0}
              y={0}
              width={width}
              height={RULER}
              className="cursor-col-resize fill-zinc-100 dark:fill-zinc-900"
              onPointerDown={(e) => {
                (e.target as Element).setPointerCapture(e.pointerId);
                drag.current = { from: secAt(e), moved: false };
              }}
              onPointerMove={(e) => {
                if (!drag.current) return;
                const t = secAt(e);
                if (Math.abs(t - drag.current.from) * zoom > 4) drag.current.moved = true;
                if (drag.current.moved) setSelecting([Math.min(drag.current.from, t), Math.max(drag.current.from, t)]);
              }}
              onPointerUp={(e) => {
                const d = drag.current;
                drag.current = null;
                setSelecting(null);
                if (!d) return;
                const t = secAt(e);
                if (d.moved && Math.abs(t - d.from) >= 0.5) onLoop([Math.min(d.from, t), Math.max(d.from, t)]);
                else onSeek(d.from);
              }}
            />
            {bars.length > 0 ? (
              <BarGrid bars={bars} zoom={zoom} ruler={RULER} bottom={height} />
            ) : (
              ticks.map((t) => (
                <g key={t} pointerEvents="none">
                  <line x1={x(t)} x2={x(t)} y1={RULER - 6} y2={height} className="stroke-zinc-200 dark:stroke-zinc-800" />
                  <text x={x(t) + 3} y={13} className="fill-zinc-500 text-[10px]">
                    {fmt(t)}
                  </text>
                </g>
              ))
            )}
            {shownLoop && (
              <rect
                x={x(shownLoop[0])}
                y={0}
                width={Math.max(1, x(shownLoop[1]) - x(shownLoop[0]))}
                height={height}
                pointerEvents="none"
                className="fill-amber-400/15 stroke-amber-500/60"
              />
            )}
            {parts.map((p, i) => {
              const y0 = RULER + i * LANE;
              const pitches = p.notes.map((n) => n.pitch);
              const lo = Math.min(...pitches);
              const hi = Math.max(...pitches);
              const span = Math.max(6, hi - lo);
              const noteH = 5;
              const top = y0 + 4;
              const room = LANE - 22 - noteH;
              const yOf = (pitch: number) => top + room * (1 - (pitch - lo) / span);
              const mine = p.name === highlight;
              const checked = check?.part === p.name ? check : null;
              // the sung pitch, broken where the voice stops
              const segments: [number, number][][] = [];
              if (checked) {
                let prev = -1;
                for (const pt of checked.line) {
                  if (pt[0] - prev > 0.05 || segments.length === 0) segments.push([]);
                  segments[segments.length - 1].push(pt);
                  prev = pt[0];
                }
              }
              const clampY = (m: number) => Math.max(y0 + 1, Math.min(y0 + LANE - 16, yOf(m) + noteH / 2));
              return (
                <g key={p.name}>
                  <rect
                    x={0}
                    y={y0}
                    width={width}
                    height={LANE}
                    className={mine ? "fill-violet-500/5" : "fill-transparent"}
                    onPointerDown={(e) => onSeek(secAt(e))}
                  />
                  <line x1={0} x2={width} y1={y0} y2={y0} className="stroke-zinc-200 dark:stroke-zinc-800" pointerEvents="none" />
                  {p.notes.map((n, k) => (
                    <g key={k} pointerEvents="none">
                      <rect
                        x={x(n.start)}
                        y={yOf(n.pitch)}
                        width={Math.max(2, x(n.end) - x(n.start) - 1)}
                        height={noteH}
                        rx={2}
                        className={
                          checked?.verdicts[n.id] && checked.verdicts[n.id] !== "not_judged"
                            ? VERDICT_CLASS[checked.verdicts[n.id]]
                            : mine
                              ? "fill-violet-600"
                              : p.sung
                                ? "fill-zinc-400 dark:fill-zinc-500"
                                : "fill-zinc-300 dark:fill-zinc-700"
                        }
                      />
                      {p.sung && n.lyric && zoom >= 25 && (
                        <text
                          x={x(n.start) + 1}
                          y={y0 + LANE - 6}
                          className={`text-[10px] ${mine ? "fill-violet-800 dark:fill-violet-200" : "fill-zinc-500"}`}
                        >
                          {n.lyric}
                        </text>
                      )}
                    </g>
                  ))}
                  {segments.map((seg, k) => (
                    <polyline
                      key={`line${k}`}
                      points={seg.map(([t, m]) => `${x(t).toFixed(1)},${clampY(m).toFixed(1)}`).join(" ")}
                      className="fill-none stroke-sky-500"
                      strokeWidth={1.5}
                      pointerEvents="none"
                    />
                  ))}
                </g>
              );
            })}
            <line x1={x(cursor)} x2={x(cursor)} y1={0} y2={height} pointerEvents="none" className="stroke-red-500" strokeWidth={1.5} />
          </svg>
        </div>
      </div>
    </div>
  );
}
