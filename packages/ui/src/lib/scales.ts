/** Minimal scale and tick helpers for SVG charts. */

export interface Scale {
  (value: number): number;
  readonly domain: readonly [number, number];
  readonly range: readonly [number, number];
  ticks(count?: number): number[];
}

export function niceStep(span: number, count: number): number {
  const raw = span / Math.max(count, 1);
  if (raw <= 0 || !Number.isFinite(raw)) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const normalized = raw / magnitude;
  const step = normalized >= 7.5 ? 10 : normalized >= 3.5 ? 5 : normalized >= 1.5 ? 2 : 1;
  return step * magnitude;
}

export function niceTicks(min: number, max: number, count = 5): number[] {
  if (!(max > min)) return [min];
  const step = niceStep(max - min, count);
  const ticks: number[] = [];
  for (let value = Math.ceil(min / step) * step; value <= max + step * 1e-9; value += step) {
    ticks.push(Number(value.toPrecision(12)));
  }
  return ticks;
}

/** Rounds `max` up to a tick boundary. */
export function niceMax(max: number, count = 5): number {
  if (!(max > 0)) return 1;
  const step = niceStep(max, count);
  return Math.ceil(max / step) * step;
}

/** Expands a positive range to whole powers of ten. */
export function logDomain(min: number, max: number): [number, number] {
  const lo = Math.max(min, Number.MIN_VALUE);
  const hi = Math.max(max, lo);
  const d0 = 10 ** Math.floor(Math.log10(lo));
  let d1 = 10 ** Math.ceil(Math.log10(hi));
  if (d1 === d0) d1 = d0 * 10;
  return [d0, d1];
}

export function linearScale(domain: [number, number], range: [number, number]): Scale {
  const [d0, d1] = domain;
  const [r0, r1] = range;
  const span = d1 - d0 || 1;
  const scale = ((value: number) => r0 + ((value - d0) / span) * (r1 - r0)) as Scale;
  return Object.assign(scale, {
    domain,
    range,
    ticks: (count = 5) => niceTicks(d0, d1, count),
  });
}

export function logScale(domain: [number, number], range: [number, number]): Scale {
  const l0 = Math.log10(domain[0]);
  const l1 = Math.log10(domain[1]);
  const [r0, r1] = range;
  const span = l1 - l0 || 1;
  const scale = ((value: number) =>
    r0 + ((Math.log10(Math.max(value, domain[0])) - l0) / span) * (r1 - r0)) as Scale;
  return Object.assign(scale, {
    domain,
    range,
    ticks: () => {
      const ticks: number[] = [];
      for (let exp = Math.ceil(l0 - 1e-9); exp <= Math.floor(l1 + 1e-9); exp++) ticks.push(10 ** exp);
      return ticks;
    },
  });
}

/**
 * Horizontal bar path from `x0` (baseline, square corners) to `x1` (data end,
 * rounded corners of radius `r`).
 */
export function horizontalBarPath(x0: number, x1: number, y: number, height: number, r = 4): string {
  const width = Math.max(0, x1 - x0);
  const radius = Math.min(r, width, height / 2);
  const yb = y + height;
  return [
    `M${x0},${y}`,
    `H${x1 - radius}`,
    `Q${x1},${y} ${x1},${y + radius}`,
    `V${yb - radius}`,
    `Q${x1},${yb} ${x1 - radius},${yb}`,
    `H${x0}`,
    "Z",
  ].join(" ");
}

/** Vertical column path from baseline `y0` (square) up to `y1` (rounded top). */
export function columnPath(x: number, width: number, y0: number, y1: number, r = 4): string {
  const height = Math.max(0, y0 - y1);
  const radius = Math.min(r, height, width / 2);
  const xr = x + width;
  return [
    `M${x},${y0}`,
    `V${y1 + radius}`,
    `Q${x},${y1} ${x + radius},${y1}`,
    `H${xr - radius}`,
    `Q${xr},${y1} ${xr},${y1 + radius}`,
    `V${y0}`,
    "Z",
  ].join(" ");
}
