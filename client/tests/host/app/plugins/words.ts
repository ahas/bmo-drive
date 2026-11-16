import { createI18n } from "vue-i18n";

import ko from "../../../../i18n/ko.json";

// The words a console hands the bundle's screens: the bundle's own pack, in the locale the console speaks. The
// layer's own keys are the console's words and stand outside this repository, so they show as their keys here.
export default defineNuxtPlugin((nuxtApp) => {
  nuxtApp.vueApp.use(createI18n({ legacy: false, globalInjection: true, locale: "ko", missingWarn: false, fallbackWarn: false, messages: { ko } }));
});
