import { fileURLToPath } from "node:url";

// The host a browser test stands the Files screen in: the client's Nuxt project as a layer, under the one app
// component a console wraps its screens in, so the screen meets the providers it meets in a console. A test
// finds what it drives by role, address and the screen's data attributes.
export default defineNuxtConfig({
  extends: ["../.."],
  devtools: { enabled: false },
  // A console renders a bundle's screen in the browser alone, and a server rendering the screen would ask the
  // stand-in's routes from the server, where the test's browser context does not stand behind them.
  ssr: false,
  // The icons come from the collections installed with the project, through the dev server, and never from the
  // network, which a test does not stand on.
  icon: { provider: "server", fallbackToApi: false },
  // The bundler's cache stays inside the client's own installed packages, which the repository ignores, rather than
  // the folder the bundler would find above the host.
  vite: { cacheDir: fileURLToPath(new URL("../../node_modules/.cache/vite-host", import.meta.url)) }
});
