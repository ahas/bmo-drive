// Files coming into the open folder of the Files screen (docs/files.md, 「The Files screen」), driven in Chromium
// against a stand-in of the service: files dropped on the list and files pasted are written into the open folder
// through the content route, a picture the browser makes at the paste is named for the paste's
// moment, a paste into a field stays the field's text, and a paste with no file in it carries the entries copied
// on the screen, which are then the newer copy over a picture on the system clipboard.

import { afterAll, beforeAll, expect, test } from "bun:test";

import { standBehind, standIn } from "../support/drive-stand-in";
import { standScreen } from "../support/screen";

import type { StandIn } from "../support/drive-stand-in";
import type { Screen } from "../support/screen";
import type { Page } from "playwright-core";

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

// Whether what is read comes to hold within the bound given.
async function until(holds: () => boolean | Promise<boolean>, ms = 15_000): Promise<boolean> {
  const end = Date.now() + ms;

  while (Date.now() < end) {
    if (await holds()) return true;

    await Bun.sleep(100);
  }

  return await holds();
}

// The screen opened at the folder `docs`, with the system clipboard the page's to read and write, handed to `drive`
// and closed with its context however `drive` ends.
async function onScreen(state: StandIn, drive: (page: Page) => Promise<void>): Promise<void> {
  const context = await screen.browser.newContext({ viewport: { width: 1400, height: 900 } });

  try {
    await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: screen.origin });
    await standBehind(context, state);

    const page = await context.newPage();

    await page.goto(`${screen.origin}/files?place=drive&path=docs`);
    await page.locator('[data-entry="readme.txt"]').first().waitFor();
    await drive(page);
    expect(state.unanswered).toEqual([]);
  } finally {
    await context.close();
  }
}

// A picture put on the system clipboard as a screenshot tool puts one, with words beside it where they are given.
async function copyPicture(page: Page, words?: string): Promise<void> {
  await page.evaluate(async (text) => {
    const canvas = document.createElement("canvas");

    canvas.width = 2;
    canvas.height = 2;

    const picture = await new Promise<Blob>((done) => canvas.toBlob((made) => done(made as Blob), "image/png"));
    const kinds: Record<string, Blob> = { "image/png": picture };

    if (text !== undefined) kinds["text/plain"] = new Blob([text], { type: "text/plain" });

    await navigator.clipboard.write([new ClipboardItem(kinds)]);
  }, words);
}

// Files copied in a file manager, pasted on the screen: a paste whose clipboard carries them, each bearing the time
// it was last changed, long before the paste.
async function pasteCopiedFiles(page: Page, files: Array<{ name: string; text: string }>): Promise<void> {
  await page.evaluate((copied) => {
    const transfer = new DataTransfer();

    for (const one of copied) transfer.items.add(new File([one.text], one.name, { type: "text/plain", lastModified: Date.UTC(2026, 0, 1) }));

    document.body.dispatchEvent(new ClipboardEvent("paste", { clipboardData: transfer, bubbles: true, cancelable: true }));
  }, files);
}

test(
  "files dropped on the open folder's list are written into it",
  async () => {
    const state = standIn();

    await onScreen(state, async (page) => {
      const list = page.locator('[data-entry="readme.txt"]').first();

      await list.evaluate((row) => {
        const transfer = new DataTransfer();

        transfer.items.add(new File(["first notes"], "notes.txt", { type: "text/plain" }));
        transfer.items.add(new File(["more"], "more.txt", { type: "text/plain" }));

        for (const kind of ["dragenter", "dragover", "drop"]) row.dispatchEvent(new DragEvent(kind, { dataTransfer: transfer, bubbles: true, cancelable: true }));
      });

      expect(await until(() => state.uploads.length === 2)).toBe(true);
      expect(state.uploads).toEqual([
        { folder: "docs", name: "notes.txt", conflict: "refuse", bytes: 11 },
        { folder: "docs", name: "more.txt", conflict: "refuse", bytes: 4 }
      ]);
      await page.locator('[data-entry="notes.txt"]').first().waitFor();
    });
  },
  DRIVING_MS
);

test(
  "files pasted are written into the open folder under their own names",
  async () => {
    const state = standIn();

    await onScreen(state, async (page) => {
      await pasteCopiedFiles(page, [{ name: "report.txt", text: "quarterly" }]);

      expect(await until(() => state.uploads.length === 1)).toBe(true);
      expect(state.uploads).toEqual([{ folder: "docs", name: "report.txt", conflict: "refuse", bytes: 9 }]);
    });
  },
  DRIVING_MS
);

test(
  "a picture the browser makes at the paste is named for the paste's moment",
  async () => {
    const state = standIn();

    await onScreen(state, async (page) => {
      await copyPicture(page);
      await page.keyboard.press("ControlOrMeta+V");

      expect(await until(() => state.uploads.length === 1)).toBe(true);
      expect(state.uploads[0]?.folder).toBe("docs");
      expect(state.uploads[0]?.name).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}\.\d{2}\.\d{2}\.png$/);
    });
  },
  DRIVING_MS
);

test(
  "a paste into the rename field stays the field's text and uploads nothing",
  async () => {
    const state = standIn();

    await onScreen(state, async (page) => {
      await copyPicture(page, "renamed");
      await page.locator('[data-entry="readme.txt"]').first().click();
      await page.keyboard.press("F2");

      const field = page.locator('[data-entry="readme.txt"] input').first();

      await field.waitFor();
      await field.fill("");
      await page.keyboard.press("ControlOrMeta+V");
      expect(await until(async () => (await field.inputValue()) === "renamed")).toBe(true);
      await page.keyboard.press("Escape");

      // A paste the screen does read, after it: its upload arriving says every paste before it was read too.
      await pasteCopiedFiles(page, [{ name: "after.txt", text: "after" }]);
      expect(await until(() => state.uploads.length > 0)).toBe(true);
      expect(state.uploads.map((one) => one.name)).toEqual(["after.txt"]);
    });
  },
  DRIVING_MS
);

test(
  "a paste with no file in it pastes the entries copied on the screen, the newer copy over a picture copied before",
  async () => {
    const state = standIn();

    await onScreen(state, async (page) => {
      await copyPicture(page);
      await page.locator('[data-entry="readme.txt"]').first().click();
      await page.keyboard.press("ControlOrMeta+C");
      // The copy's paths reach the system clipboard on the browser's own time, as a person's next key never outruns.
      expect(await until(async () => (await page.evaluate(() => navigator.clipboard.readText())) !== "")).toBe(true);
      await page.keyboard.press("ControlOrMeta+V");

      expect(await until(() => state.copies.length === 1)).toBe(true);
      expect(state.copies).toEqual([{ paths: ["docs/readme.txt"], to: "docs" }]);
      expect(state.uploads).toEqual([]);
    });
  },
  DRIVING_MS
);
