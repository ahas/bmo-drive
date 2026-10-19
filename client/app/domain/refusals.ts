import type { Translate } from "bmo-ui/shared/wording";

// What the screen says of a call the service refused (docs/api.md, "Refusals"): the service answers an
// English sentence and a code, and the screen says the code in the language pack's words under
// `files.error.<code>`. A call that failed with no code of the service's says the caller's own words.
export function refusalText(t: Translate, error: unknown, fallback: string): string {
  const code = (error as { data?: { code?: unknown } } | null)?.data?.code;

  return typeof code === "string" && code !== "" ? t(`files.error.${code}`) : fallback;
}

// Why a machine or Garage could not be read, as the places answer says it: its code in the language pack's
// words, or the sentence where it carries no code — one the screen wrote itself.
export function faultText(t: Translate, code: string | null, error: string): string {
  return code === null || code === "" ? error : t(`files.error.${code}`);
}
