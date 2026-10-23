import FilesPage from "../app/pages/files/index.vue";
import SharePage from "../app/pages/files/share/[token].vue";

import "./screens.css";

import type { ConsoleBundleModule } from "bmo-ui/shared/console-manifest";

// The console bundle's module (README.md, "The console plugin bundle"): the Files screen and the file
// share page under the screen path, each with the meta its page states for a Nuxt build.
const bundle: ConsoleBundleModule = {
  routes: [
    { path: "/files", name: "files", component: FilesPage },
    { path: "/files/share/:token", name: "files-share-token", meta: { layout: false }, component: SharePage }
  ]
};

export default bundle;
