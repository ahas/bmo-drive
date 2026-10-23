// A file's size as the screen writes it: bytes, kilobytes, or megabytes or gigabytes to one place.
export function fileSizeText(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes.toLocaleString("ko-KR")}B`;
  }

  if (bytes < 1024 * 1024) {
    return `${Math.round(bytes / 1024).toLocaleString("ko-KR")}KB`;
  }

  const giga = bytes >= 1024 * 1024 * 1024;
  const shown = bytes / (giga ? 1024 * 1024 * 1024 : 1024 * 1024);

  return `${shown.toLocaleString("ko-KR", { minimumFractionDigits: 1, maximumFractionDigits: 1 })}${giga ? "GB" : "MB"}`;
}
