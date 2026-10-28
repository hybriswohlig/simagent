/**
 * The SVG geometry of a census curve, which only `web/src/render/` may compute: a fixed box,
 * x linear in the year, y linear between the ends of an axis whose step is the smallest of
 * 1, 2 and 5 times a power of ten, not below the shown decimals, that needs at most five
 * steps. A line breaks at a year without a point; a lone value is a dot. The axis is worked
 * out in whole units of the last shown decimal (tenths of a cm, say), where binary64 is
 * exact: 160.1 / 0.1 lies below 1601.
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

/** The whole steps 1, 2 and 5 times a power of ten, from 1 up. */
function* steps(): Generator<number> {
  for (let power = 1; ; power *= 10) {
    for (const m of [1, 2, 5]) {
      yield m * power;
    }
  }
}

/** The axis of `units` (whole units): its first step count from `lo / step`, its step and
 * its count; `none` the step of an axis without values. */
function axis(units: number[], fromZero: boolean, none: number): { base: number; step: number; count: number } {
  if (units.length === 0) {
    return { base: 0, step: none, count: 1 };
  }
  const min = Math.min(...units);
  const max = Math.max(...units);
  for (const step of steps()) {
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
  for (const step of steps()) {
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

/** The x of a year on the plot of `years`: linear from the first to the last; one year in the
 * middle. */
function xScale(years: readonly number[]): (year: number) => number {
  const first = years[0] ?? 0;
  const span = (years[years.length - 1] ?? 0) - first;
  return (year) => (span === 0 ? LEFT + PLOT_WIDTH / 2 : LEFT + (PLOT_WIDTH * (year - first)) / span);
}

/** A band behind the lines, from `x1` to `x2`. */
export type Band = { x1: number; x2: number };

/** The `marked` years among `years` as bands (Grok round 40): each run of marked years that
 * follow each other, from halfway to the year before to halfway to the year after, cut to the
 * plot; one year shown fills the plot. */
export function bands(years: readonly number[], marked: readonly number[]): Band[] {
  const shown = years.filter((year) => marked.includes(year));
  if (shown.length > 0 && years.length === 1) {
    return [{ x1: LEFT, x2: RIGHT }];
  }
  const x = xScale(years);
  const cut = (year: number) => Math.min(RIGHT, Math.max(LEFT, x(year)));
  const runs: [number, number][] = [];
  for (const year of shown) {
    const last = runs[runs.length - 1];
    if (last !== undefined && year === last[1] + 1) {
      last[1] = year;
    } else {
      runs.push([year, year]);
    }
  }
  return runs.map(([from, to]) => ({ x1: cut(from - 0.5), x2: cut(to + 0.5) }));
}

/** The curve of `lines` over `years` (one value or `null` per year and line, with at most
 * `decimals` decimals). */
export function chart(
  years: readonly number[],
  lines: readonly (readonly (number | null)[])[],
  fromZero: boolean,
  decimals: number,
): Chart {
  const x = xScale(years);
  const scale = 10 ** decimals;
  const unit = (value: number) => Math.round(value * scale);
  const shown = lines.flatMap((line) => line.filter((value): value is number => value !== null));
  const { base, step, count } = axis(shown.map(unit), fromZero, scale);
  const lo = base * step;
  const hi = (base + count) * step;
  const y = (value: number) => TOP + PLOT_HEIGHT * (1 - (unit(value) - lo) / (hi - lo));
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
    const units = (base + i) * step;
    return { value: units / scale, at: TOP + PLOT_HEIGHT * (1 - (units - lo) / (hi - lo)) };
  });
  return { width: WIDTH, height: HEIGHT, segments, dots, yTicks, xTicks: yearTicks(years, x) };
}
