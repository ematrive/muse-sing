import { useRef, useState } from "react";
import { noteName, type Phrase } from "../../lib/api";

const W = 1000;
const RULER = 16;
const H = 150;
const LYRICS = 24;

/** Start beat of every note, plus the total length in beats. */
export function noteStarts(phrase: Phrase) {
  const starts: number[] = [];
  let pos = 0;
  for (const n of phrase.notes) {
    starts.push(pos);
    pos += n.beats;
  }
  return { starts, total: pos };
}

/** Index of the note sounding at `beats` (-1 if none). */
export function noteAt(phrase: Phrase, beats: number) {
  const { starts } = noteStarts(phrase);
  for (let i = starts.length - 1; i >= 0; i--) if (beats >= starts[i]) return beats < starts[i] + phrase.notes[i].beats ? i : -1;
  return -1;
}

/**
 * Notes of a phrase. With `interactive`, clicking a note calls `onNote`, clicking or dragging
 * anywhere else (or on the ruler) moves the cursor through `onSeek` (`commit` on release).
 */
export function PianoRoll({
  phrase,
  cursor,
  playing,
  highlight,
  interactive,
  onSeek,
  onNote,
}: {
  phrase: Phrase;
  /** Start position (beats), shown when nothing plays. */
  cursor: number;
  /** Playback position (beats) while something plays, null otherwise. */
  playing: number | null;
  /** Note to highlight (the one sounding or just clicked), -1 for none. */
  highlight: number;
  interactive: boolean;
  onSeek: (beats: number, commit: boolean) => void;
  onNote: (i: number) => void;
}) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [dragging, setDragging] = useState(false);
  const [hover, setHover] = useState(-1);
  const pitches = phrase.notes.map((n) => n.pitch);
  const lo = Math.min(...pitches) - 2;
  const hi = Math.max(...pitches) + 2;
  const { starts, total } = noteStarts(phrase);
  const rowH = H / (hi - lo + 1);
  const x = (b: number) => (b / total) * W;

  const beatsAt = (clientX: number) => {
    const r = svgRef.current!.getBoundingClientRect();
    const b = ((clientX - r.left) / r.width) * total;
    // snap to a note start when close (8 px), so it is easy to start exactly on a note
    const snap = (8 / r.width) * total;
    const near = [...starts, total].find((s) => Math.abs(s - b) < snap);
    return Math.max(0, Math.min(total, near ?? b));
  };

  const down = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!interactive || e.button !== 0) return;
    const note = (e.target as Element).closest("[data-note]")?.getAttribute("data-note");
    if (note != null) {
      onNote(+note);
      return;
    }
    svgRef.current?.setPointerCapture(e.pointerId);
    setDragging(true);
    onSeek(beatsAt(e.clientX), false);
  };
  const move = (e: React.PointerEvent<SVGSVGElement>) => {
    if (dragging) onSeek(beatsAt(e.clientX), false);
  };
  const up = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!dragging) return;
    setDragging(false);
    onSeek(beatsAt(e.clientX), true);
  };

  const showPlay = playing !== null && playing >= 0 && playing <= total;
  // the start cursor only matters when it can be used (not while recording)
  const head = showPlay ? playing : playing === null && interactive ? cursor : null;

  return (
    <svg
      ref={svgRef}
      viewBox={`0 0 ${W} ${RULER + H + LYRICS}`}
      className={`w-full touch-none select-none ${interactive ? (dragging ? "cursor-grabbing" : "cursor-pointer") : ""}`}
      preserveAspectRatio="none"
      style={{ height: 206 }}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={() => setDragging(false)}
    >
      {/* ruler: one tick per beat */}
      <rect x={0} y={0} width={W} height={RULER - 4} rx={3} className="fill-zinc-100 dark:fill-zinc-800/80" />
      {Array.from({ length: Math.floor(total) + 1 }, (_, b) => (
        <line key={b} x1={x(b)} x2={x(b)} y1={b % 4 === 0 ? 1 : 5} y2={RULER - 5} className="stroke-zinc-400 dark:stroke-zinc-600" strokeWidth={1.5} />
      ))}
      <g transform={`translate(0 ${RULER})`}>
        {Array.from({ length: hi - lo + 1 }, (_, i) => {
          const p = hi - i;
          const black = [1, 3, 6, 8, 10].includes(((p % 12) + 12) % 12);
          return <rect key={p} x={0} y={i * rowH} width={W} height={rowH} className={black ? "fill-zinc-100 dark:fill-zinc-800/60" : "fill-transparent"} />;
        })}
        {phrase.notes.map((n, i) => {
          const x0 = x(starts[i]);
          const w = Math.max(2, x(n.beats) - 3);
          const y = (hi - n.pitch) * rowH;
          const lit = i === highlight;
          const fill = lit
            ? "fill-amber-400 dark:fill-amber-300"
            : n.syl
              ? hover === i && interactive
                ? "fill-violet-400"
                : "fill-violet-500"
              : "fill-violet-300 dark:fill-violet-700";
          return (
            <g key={i}>
              <rect
                data-note={i}
                x={x0 + 1}
                y={y + 1}
                width={w}
                height={rowH - 2}
                rx={3}
                className={`${fill} transition-colors`}
                onPointerEnter={() => setHover(i)}
                onPointerLeave={() => setHover(-1)}
              >
                <title>
                  {noteName(n.pitch)}
                  {n.syl ? ` · «${n.syl}»` : " · (prosegue la vocale)"}
                  {interactive ? " — clic per ascoltarla" : ""}
                </title>
              </rect>
              {n.syl && (
                <text
                  x={x0 + 2}
                  y={H + 17}
                  className={lit ? "fill-amber-600 font-semibold dark:fill-amber-300" : "fill-zinc-700 dark:fill-zinc-300"}
                  style={{ fontSize: 15 }}
                >
                  {n.syl}
                </text>
              )}
            </g>
          );
        })}
      </g>
      {head !== null && (
        <g pointerEvents="none">
          <line
            x1={x(head)}
            x2={x(head)}
            y1={RULER - 4}
            y2={RULER + H}
            className={showPlay ? "stroke-red-500" : "stroke-zinc-500 dark:stroke-zinc-300"}
            strokeWidth={showPlay ? 3 : 2}
            strokeDasharray={showPlay ? undefined : "6 4"}
          />
          <path
            d={`M ${x(head) - 7} 0 L ${x(head) + 7} 0 L ${x(head)} ${RULER - 2} Z`}
            className={showPlay ? "fill-red-500" : "fill-zinc-500 dark:fill-zinc-300"}
          />
        </g>
      )}
    </svg>
  );
}
