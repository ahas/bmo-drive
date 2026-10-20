// The spans a share link may be made to live, each with the key its name is read by in the language pack
// and how many seconds it lives from the moment it is made. The screen works the link's end out of the
// span it is asked for; one span has no end, and is ended by taking the link away.
export const SHARE_SPANS = [
  { key: "1h", label: "drive.label.span_1h", seconds: 60 * 60 },
  { key: "6h", label: "drive.label.span_6h", seconds: 6 * 60 * 60 },
  { key: "24h", label: "drive.label.span_24h", seconds: 24 * 60 * 60 },
  { key: "7d", label: "drive.label.span_7d", seconds: 7 * 24 * 60 * 60 },
  { key: "30d", label: "drive.label.span_30d", seconds: 30 * 24 * 60 * 60 },
  { key: "forever", label: "common.label.forever", seconds: null }
] as const;

export type ShareSpanKey = (typeof SHARE_SPANS)[number]["key"];

/** How many seconds a span's key lets a link live, or null for no end. */
export function spanSeconds(key: string): number | null {
  return SHARE_SPANS.find((span) => span.key === key)?.seconds ?? null;
}
