// The drive console bundle as a layer: the Files screen and the file share page, their components and the
// modules they stand on, on top of bmo-ui. A console extends it as it extends bmo-ui and attaches it by its
// declaration, `console-bundle.json` (README.md, "The console plugin bundle").
export default defineNuxtConfig({
  extends: ["bmo-ui"],
  compatibilityDate: "2025-09-01",
  devtools: { enabled: false }
});
