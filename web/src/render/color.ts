/**
 * The melanin rule of the body contract (ADR-0005 section 5): a reader linearises the sRGB
 * table colours (IEC 61966-2-1), interpolates bilinearly over eumelanin and pheomelanin
 * (skin, hair) or linearly over melanin (iris), and converts back. A value outside [0, 1] is
 * clamped; a channel is rounded half up to 8 bits.
 */

/** A table over two axes: `srgb[i][j]` belongs to `eumelanin[i]` and `pheomelanin[j]`. */
export type Grid = {
  readonly eumelanin: readonly number[];
  readonly pheomelanin: readonly number[];
  readonly srgb: readonly (readonly string[])[];
};

/** A table over one axis: `srgb[i]` belongs to `melanin[i]`. */
export type Ramp = {
  readonly melanin: readonly number[];
  readonly srgb: readonly string[];
};

type Linear = readonly [number, number, number];

function clamp(value: number): number {
  return Math.min(1, Math.max(0, value));
}

function toLinear(channel: number): number {
  return channel <= 0.04045 ? channel / 12.92 : Math.pow((channel + 0.055) / 1.055, 2.4);
}

function toSrgb(linear: number): number {
  const value = linear <= 0.0031308 ? 12.92 * linear : 1.055 * Math.pow(linear, 1 / 2.4) - 0.055;
  return clamp(value);
}

/** The linear channels of a colour `#rrggbb`. */
export function linear(hex: string): Linear {
  const byte = (at: number) => Number.parseInt(hex.slice(at, at + 2), 16) / 255;
  return [toLinear(byte(1)), toLinear(byte(3)), toLinear(byte(5))];
}

/** The colour `#rrggbb` of linear channels. */
export function hex(channels: Linear): string {
  const byte = (channel: number) => Math.floor(toSrgb(channel) * 255 + 0.5);
  return "#" + channels.map((c) => byte(c).toString(16).padStart(2, "0")).join("");
}

/** The cell of `points` around `value` (the last cell for the last point) and the share of
 * the way through it. */
function cell(points: readonly number[], value: number): [number, number] {
  const last = points.length - 2;
  let i = 0;
  while (i < last && value > (points[i + 1] ?? 1)) {
    i += 1;
  }
  const low = points[i] ?? 0;
  const high = points[i + 1] ?? 1;
  return [i, (value - low) / (high - low)];
}

function mix(a: Linear, b: Linear, share: number): Linear {
  const at = (k: 0 | 1 | 2) => a[k] * (1 - share) + b[k] * share;
  return [at(0), at(1), at(2)];
}

/** The colour of skin or hair at `eumelanin` and `pheomelanin`. */
export function gridColor(grid: Grid, eumelanin: number, pheomelanin: number): string {
  const [i, t] = cell(grid.eumelanin, clamp(eumelanin));
  const [j, u] = cell(grid.pheomelanin, clamp(pheomelanin));
  const at = (a: number, b: number) => linear(grid.srgb[a]?.[b] ?? "#000000");
  const low = mix(at(i, j), at(i, j + 1), u);
  const high = mix(at(i + 1, j), at(i + 1, j + 1), u);
  return hex(mix(low, high, t));
}

/** The colour of the iris at `melanin`. */
export function rampColor(ramp: Ramp, melanin: number): string {
  const [i, t] = cell(ramp.melanin, clamp(melanin));
  const at = (a: number) => linear(ramp.srgb[a] ?? "#000000");
  return hex(mix(at(i), at(i + 1), t));
}
