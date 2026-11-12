// The Nuxt configuration the bundle's typecheck runs on: the Files screen and the file share page, their
// components and the modules they stand on, on top of bmo-ui. The served bundle is built by the build in
// `console/` (README.md, "The console plugin bundle").
export default defineNuxtConfig({
  extends: ["bmo-ui"],
  compatibilityDate: "2025-09-01",
  devtools: { enabled: false }
});
