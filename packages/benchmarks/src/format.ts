/** Display formatters for benchmark values. All are locale-stable (en-US). */

const integer = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });

function trim(value: number, digits: number): string {
  return new Intl.NumberFormat("en-US", {
    maximumFractionDigits: digits,
    minimumFractionDigits: 0,
  }).format(value);
}

/** Adaptive precision: 3 significant-ish digits. */
function adaptive(value: number): string {
  const abs = Math.abs(value);
  if (abs >= 100) return integer.format(value);
  if (abs >= 10) return trim(value, 1);
  return trim(value, 2);
}

/** Formats a microsecond duration as µs, ms or s. */
export function formatMicros(us: number): string {
  if (!Number.isFinite(us)) return "–";
  if (us < 1_000) return `${adaptive(us)} µs`;
  if (us < 1_000_000) return `${adaptive(us / 1_000)} ms`;
  return `${adaptive(us / 1_000_000)} s`;
}

/** Formats a millisecond duration as µs, ms, s or min. */
export function formatMillis(ms: number): string {
  if (!Number.isFinite(ms)) return "–";
  if (ms < 1) return formatMicros(ms * 1_000);
  if (ms < 1_000) return `${adaptive(ms)} ms`;
  if (ms < 60_000) return `${adaptive(ms / 1_000)} s`;
  return `${adaptive(ms / 60_000)} min`;
}

/** Formats a byte count using binary units. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return "–";
  if (bytes < 1024) return `${integer.format(bytes)} B`;
  if (bytes < 1024 * 1024) return `${adaptive(bytes / 1024)} KB`;
  return `${adaptive(bytes / (1024 * 1024))} MB`;
}

/** Compact counts: 1,284 / 12.9K / 4.2M. */
export function formatCount(value: number): string {
  if (!Number.isFinite(value)) return "–";
  const abs = Math.abs(value);
  if (abs < 10_000) return integer.format(value);
  if (abs < 1_000_000) return `${trim(value / 1_000, 1)}K`;
  return `${trim(value / 1_000_000, 1)}M`;
}

/** Formats a percentage value (already ×100). */
export function formatPercent(pct: number, digits = 0): string {
  if (!Number.isFinite(pct)) return "–";
  return `${trim(pct, digits)}%`;
}

/** Signed multiplier, e.g. "3.2×". */
export function formatRatio(ratio: number): string {
  if (!Number.isFinite(ratio)) return "–";
  return `${adaptive(ratio)}×`;
}

/** Formats an epoch-millisecond timestamp as an absolute UTC date. */
export function formatDate(ms: number): string {
  return new Intl.DateTimeFormat("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    timeZone: "UTC",
    timeZoneName: "short",
  }).format(new Date(ms));
}
