// The Files screen's view chosen from the toolbar (docs/files.md, 「The Files screen」), driven in Chromium against a
// stand-in of the service at a wide window and at a phone's width: the view is bmo-ui's one select, whose eight
// views open on a search, and the view found and chosen there is the one the screen's address then names.
// Whether a list searches is the layer's own rule and its own tests; this holds the screen's list to it.

import { afterAll, beforeAll, expect, test } from "bun:test";

import words from "../../i18n/ko.json";
import { standBehind, standIn } from "../support/drive-stand-in";
import { standScreen } from "../support/screen";

import type { Screen } from "../support/screen";

let screen: Screen;

// What a test may take at most, a bound and never a wait: the dev server builds the screen on its first page.
const STANDING_MS = 300_000;
const DRIVING_MS = 120_000;

beforeAll(async () => {
  screen = await standScreen();
}, STANDING_MS);

afterAll(async () => {
  await screen?.close();
});

for (const width of [
  { name: "a wide window", viewport: { width: 1280, height: 900 } },
  { name: "a phone", viewport: { width: 360, height: 780 } },
]) {
  test(`at ${width.name}, the view select opens its eight views on a search and the view found there is the screen's`, async () => {
    const state = standIn();
    const context = await screen.browser.newContext({ viewport: width.viewport });

    try {
      await standBehind(context, state);

      const page = await context.newPage();

      await page.goto(`${screen.origin}/files?place=drive&path=docs`);
      await page.locator('[data-entry="readme.txt"]').first().waitFor();

      await page.locator(`button[aria-label="${words.drive.action.view}"]`).first().click();

      const list = page.locator('[data-slot="content"][data-side]:has([role="listbox"])');

      await list.waitFor();
      expect(await list.getByRole("option").count()).toBe(8);
      await list.locator("input").fill(words.drive.label.view_list);
      await list.getByRole("option", { name: words.drive.label.view_list, exact: true }).click();
      await page.waitForURL((url) => url.searchParams.get("view") === "list");
      expect(state.unanswered).toEqual([]);
    } finally {
      await context.close();
    }
  }, DRIVING_MS);
}
