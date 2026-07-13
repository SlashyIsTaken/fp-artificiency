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

/// Tool-result sizes are measured in characters; ~4 chars ≈ 1 token.
export const estTokens = (chars: number) => Math.round(chars / 4);
