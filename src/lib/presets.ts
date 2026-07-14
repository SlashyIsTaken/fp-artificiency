import type { RangePreset } from "./components/RangeSelector.svelte";

export const PRESETS: RangePreset[] = [
  { label: "1h", hours: 1, bucket: "minute" },
  { label: "24h", hours: 24, bucket: "hour" },
  { label: "7d", hours: 24 * 7, bucket: "day" },
  { label: "30d", hours: 24 * 30, bucket: "day" },
  { label: "3mo", hours: 24 * 91, bucket: "day" },
  { label: "All", hours: 0, bucket: "day" },
];

export const DEFAULT_PRESET = PRESETS[3]; // 30d

const compact = new Intl.NumberFormat("en", {
  notation: "compact",
  maximumFractionDigits: 1,
});

export const fmt = (n: number) => compact.format(n);

// Cost is an owned estimate, not a bill (see pricing.rs). Sub-cent spend still
// reads as "$0.00" rather than "$0" so a priced-but-tiny range looks priced;
// larger figures round to cents.
const usd = new Intl.NumberFormat("en", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 2,
});
export const money = (n: number) => usd.format(n);

/// Tool-result sizes are measured in characters; ~4 chars ≈ 1 token.
export const estTokens = (chars: number) => Math.round(chars / 4);
