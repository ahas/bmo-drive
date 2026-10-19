// The spans a share link may be made to live, by the key the share API takes, each with the key its name
// is read by in the language pack. The service works the expiry out of the key, so the screen never
// hands over a time of its own; one span has no end, and is ended by taking the link away.
export const SHARE_SPANS = [
  { key: "1h", label: "common.label.span_1h" },
  { key: "6h", label: "common.label.span_6h" },
  { key: "24h", label: "common.label.span_24h" },
  { key: "7d", label: "common.label.span_7d" },
  { key: "30d", label: "common.label.span_30d" },
  { key: "forever", label: "common.label.forever" }
] as const;

export type ShareSpanKey = (typeof SHARE_SPANS)[number]["key"];
