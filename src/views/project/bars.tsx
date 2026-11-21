import { memo } from "react";
import type { RollBar } from "../../lib/api";

/** Minimum room between two measure numbers, in pixels. */
const MIN_LABEL_PX = 28;

/** Every how many measures a number fits at this zoom: 1, 2, 4, 8… */
function labelStride(bars: RollBar[], zoom: number) {
  if (bars.length < 2) return 1;
  const avg = ((bars[bars.length - 1].start - bars[0].start) / (bars.length - 1)) * zoom;
  let stride = 1;
  while (avg * stride < MIN_LABEL_PX && stride < 64) stride *= 2;
  return stride;
}

/** The measure (printed number) that holds `tick`, if any. */
export function barAt(bars: RollBar[], tick: number): RollBar | undefined {
  let found: RollBar | undefined;
  for (const b of bars) {
    if (b.tick > tick) break;
    found = b;
  }
  return found;
}

/**
 * Bar lines with the measure numbers of the score on the ruler, and the time signature where it
 * changes. Numbers thin out when the measures get narrow.
 */
export const BarGrid = memo(function BarGrid({
  bars,
  zoom,
  ruler,
  bottom,
}: {
  bars: RollBar[];
  /** pixels per second */
  zoom: number;
  /** height of the ruler */
  ruler: number;
  /** where the lines end */
  bottom: number;
}) {
  const stride = labelStride(bars, zoom);
  let counted = 0;
  return (
    <g pointerEvents="none">
      {bars.map((b, i) => {
        const x = b.start * zoom;
        const label = b.number !== null && counted++ % stride === 0;
        return (
          <g key={i}>
            <line
              x1={x}
              x2={x}
              y1={label || b.time_sig ? 2 : ruler - 5}
              y2={bottom}
              className={label || b.time_sig ? "stroke-zinc-300 dark:stroke-zinc-700" : "stroke-zinc-200 dark:stroke-zinc-800"}
            />
            {label && (
              <text x={x + 3} y={ruler - 6} className="fill-zinc-500 text-[10px]">
                {b.number}
              </text>
            )}
            {b.time_sig && (
              <text x={x + 3} y={ruler + 10} className="fill-violet-500 text-[9px] font-semibold">
                {b.time_sig}
              </text>
            )}
          </g>
        );
      })}
    </g>
  );
});
