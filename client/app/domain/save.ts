// A file saved from its address without leaving the page. A navigation to a file the service hands over as a
// download is cancelled by anything that moves the browser's history at the same moment, and a menu does exactly
// that as it closes: the layer it stands as takes its history entry back (bmo-ui's `docs/components.md`, "Floating
// things"). A download link pressed starts the download without navigating, so nothing the menu does can stop it.
export function saveFrom(address: string): void {
  const link = document.createElement("a");

  link.href = address;
  // Empty, so the name the service answers the file under is the one it is saved as.
  link.download = "";
  link.rel = "noopener";
  link.hidden = true;
  document.body.append(link);
  link.click();
  link.remove();
}
