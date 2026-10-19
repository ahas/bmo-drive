// What the screen says of a file viewer's share link (docs/files.md, 「Share links」).

/** The link's page, under the console's own base. */
export function filesSharePage(token: string): string {
  return `/share/files/${token}`;
}
