/**
 * The SVG geometry of a census curve, which only `web/src/render/` may compute: a fixed box,
 * x linear in the year, y linear between the ends of an axis whose step is the smallest of
 * 1, 2 and 5 times a power of ten, not below the shown decimals, that needs at most five
 * steps. A line breaks at a year without a point; a lone value is a dot.
 */
export type Point = { x: number; y: number };
export type Tick = { value: number; at: number };
export type Chart = {
  width: number;
  height: number;
  /** Per line, its runs of at least two points. */
  segments: Point[][][];
  /** Per line, its points without a neighbour. */
  dots: Point[][];
  yTicks: Tick[];
  xTicks: Tick[];
};

export const WIDTH = 460;
export const HEIGHT = 200;
export const LEFT = 48;
export const TOP = 12;
const PLOT_WIDTH = 400;
const PLOT_HEIGHT = 160;
export const RIGHT = LEFT + PLOT_WIDTH;
export const BOTTOM = TOP + PLOT_HEIGHT;
const MOST_STEPS = 5;

/** The colours of the lines in their order; the first two are the women and the men, dark
 * tones of the pedigree's. */
const PALETTE = ["#b03a6f", "#2f6fb0", "#3a8f4a", "#d08a1f", "#7a4fa3", "#8a5a3c", "#2a9d9d", "#666666"];

/** The colour of the `i`-th line of a curve. */
export function lineColor(i: number): string {
  return PALETTE[i % PALETTE.length] as string;
}

/** The steps 1, 2 and 5 times a power of ten, from `10^-decimals` up. */
function* steps(decimals: number): Generator<number> {
  for (let k = -decimals; ; k += 1) {
    for (const m of [1, 2, 5]) {
      yield k < 0 ? m / 10 ** -k : m * 10 ** k;
    }
  }
}

/** The axis of the values: its first step count from `lo / step`, its step and its count. */
function axis(values: number[], fromZero: boolean, decimals: number): { base: number; step: number; count: number } {
  if (values.length === 0) {
    return { base: 0, step: 1, count: 1 };
  }
  const min = Math.min(...values);
  const max = Math.max(...values);
  for (const step of steps(decimals)) {
    const base = fromZero ? 0 : Math.floor(min / step);
    const count = Math.ceil(max / step) - base;
    if (count <= MOST_STEPS) {
      return { base, step, count: Math.max(count, 1) };
    }
  }
  throw new Error("unreachable: the steps grow without end");
}

/** The ticks of the years: the multiples of the smallest step 1, 2 or 5 times a power of ten
 * that splits the run into at most five steps. */
function yearTicks(years: readonly number[], x: (year: number) => number): Tick[] {
  const first = years[0];
  const last = years[years.length - 1];
  if (first === undefined || last === undefined) {
    return [];
  }
  for (const step of steps(0)) {
    if ((last - first) / step <= MOST_STEPS) {
      const ticks: Tick[] = [];
      for (let n = Math.ceil(first / step); n * step <= last; n += 1) {
        ticks.push({ value: n * step, at: x(n * step) });
      }
      return ticks;
    }
  }
  throw new Error("unreachable: the steps grow without end");
}

/** The curve of `lines` over `years` (one value or `null` per year and line). */
export function chart(
  years: readonly number[],
  lines: readonly (readonly (number | null)[])[],
  fromZero: boolean,
  decimals: number,
): Chart {
  const first = years[0] ?? 0;
  const span = (years[years.length - 1] ?? 0) - first;
  const x = (year: number) => (span === 0 ? LEFT + PLOT_WIDTH / 2 : LEFT + (PLOT_WIDTH * (year - first)) / span);
  const shown = lines.flatMap((line) => line.filter((value): value is number => value !== null));
  const { base, step, count } = axis(shown, fromZero, decimals);
  const lo = base * step;
  const hi = (base + count) * step;
  const y = (value: number) => TOP + PLOT_HEIGHT * (1 - (value - lo) / (hi - lo));
  const segments: Point[][][] = [];
  const dots: Point[][] = [];
  for (const line of lines) {
    const runs: Point[][] = [];
    let run: Point[] = [];
    years.forEach((year, i) => {
      const value = line[i] ?? null;
      if (value === null) {
        run = [];
      } else {
        if (run.length === 0) {
          runs.push(run);
        }
        run.push({ x: x(year), y: y(value) });
      }
    });
    segments.push(runs.filter((points) => points.length > 1));
    dots.push(runs.filter((points) => points.length === 1).flat());
  }
  const yTicks = Array.from({ length: count + 1 }, (_, i) => {
    const value = (base + i) * step;
    return { value, at: y(value) };
  });
  return { width: WIDTH, height: HEIGHT, segments, dots, yTicks, xTicks: yearTicks(years, x) };
}
