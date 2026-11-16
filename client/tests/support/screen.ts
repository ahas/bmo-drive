import { spawn } from "node:child_process";
import { createServer } from "node:net";

import { chromium } from "playwright-core";

import type { Browser } from "playwright-core";

// The Files screen standing on its own for a browser test: the client's Nuxt project served by its dev server on
// a free loopback port, the screen's page at `/files`, and a headless Chromium to drive it. The dev server runs
// in a process group of its own, so taking it down takes every process it started with it.

export interface Screen {
  origin: string;
  browser: Browser;
  close: () => Promise<void>;
}

// How long the dev server may take to answer the screen's page, which it builds on the first ask.
const READY_MS = 180_000;

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer();

    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      const port = typeof address === "object" && address !== null ? address.port : 0;

      probe.close(() => resolve(port));
    });
  });
}

// The dev server's answer to the screen's page once it gives one, asked until it does or the bound passes.
async function ready(origin: string, exited: () => boolean): Promise<void> {
  const deadline = Date.now() + READY_MS;

  while (Date.now() < deadline) {
    if (exited()) throw new Error("The drive client's dev server ended before it answered.");

    try {
      if ((await fetch(`${origin}/files`)).ok) return;
    } catch {
      // Not listening yet.
    }

    await Bun.sleep(250);
  }

  throw new Error(`The drive client's dev server did not answer ${origin}/files in time.`);
}

export async function standScreen(): Promise<Screen> {
  const port = await freePort();
  const origin = `http://127.0.0.1:${port}`;
  // The runtime running the test runs the dev server too, so no command search is asked to find it.
  const server = spawn(process.execPath, ["x", "nuxi", "dev", "--port", String(port), "--host", "127.0.0.1"], {
    cwd: new URL("../host", import.meta.url).pathname,
    detached: true,
    stdio: ["ignore", "pipe", "pipe"]
  });
  let exited = false;
  // A process that never started ends with an error and no exit.
  const ended = new Promise<void>((resolve) => {
    server.once("exit", () => resolve());
    server.once("error", () => resolve());
  });
  // What the dev server said, kept for the failure that names it; it ends at once when its output is cast aside,
  // so the output is read.
  let said = "";
  const hear = (chunk: Buffer) => {
    said = (said + chunk.toString()).slice(-4000);
  };

  server.stdout.on("data", hear);
  server.stderr.on("data", hear);
  server.once("exit", () => {
    exited = true;
  });
  server.once("error", (failure) => {
    exited = true;
    said += `\n${failure.message}`;
  });

  // The dev server's whole group taken down, and its end waited for.
  const stop = async () => {
    if (!exited && server.pid !== undefined) process.kill(-server.pid, "SIGTERM");
    await ended;
  };

  try {
    await ready(origin, () => exited);

    const browser = await chromium.launch();

    return {
      origin,
      browser,
      close: async () => {
        await browser.close();
        await stop();
      }
    };
  } catch (failure) {
    await stop();
    throw new Error(`${(failure as Error).message}\nThe dev server said:\n${said}`);
  }
}
