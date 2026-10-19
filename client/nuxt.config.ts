// The Files screen and the file share page as a layer: their pages, their components and the modules they
// stand on, on top of bmo-ui. A host extends it as it extends bmo-ui, and brings its i18n module, its
// language pack and the components this layer leaves to it (README.md, "The console plugin bundle").
export default defineNuxtConfig({
  extends: ["bmo-ui"],
  compatibilityDate: "2025-09-01",
  devtools: { enabled: false }
});
