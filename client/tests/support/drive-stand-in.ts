import type { BrowserContext, Route } from "playwright-core";

import type { components } from "../../shared/openapi";

type Wire = components["schemas"];

// A stand-in for the drive service behind the screen's relay address (docs/api.md): an owner who is no
// administrator, with the drive, its one folder `docs` and the file `readme.txt` in it, and what is uploaded
// kept, so a listing read again holds it. It answers only the routes the screen reads on opening a folder and on
// choosing a file, the upload's content route and the copy route, which it records and answers as done; any other
// is answered 404 and counted, so a test sees a route the screen began asking that the stand-in does not hold. The
// length a request goes with is the browser's network's own, set below where a test's interception reads it, so
// what the stand-in keeps of an upload is the bytes that came.

export const DRIVE = "drive";

/** One file written through the content route, as it arrived. */
export interface Upload {
  // The folder it was written into, and its name there.
  folder: string;
  name: string;
  conflict: string | null;
  bytes: number;
}

/** One copy asked for: the entries' paths and the folder they go into. */
export interface Copy {
  paths: string[];
  to: string;
}

export interface StandIn {
  uploads: Upload[];
  copies: Copy[];
  // The routes asked that the stand-in does not answer, as `METHOD path`.
  unanswered: string[];
}

export function standIn(): StandIn {
  return { uploads: [], copies: [], unanswered: [] };
}

const NOW = Date.UTC(2026, 10, 1, 9);

function entry(folder: string, name: string, kind: Wire["EntryKind"], bytes: number | null): Wire["Entry"] {
  return { name, kind, path: folder === "" ? name : `${folder}/${name}`, placeName: DRIVE, byteCount: bytes, changedAt: NOW, linkTarget: null };
}

// What stands in a folder: the folder `docs` at the top, the file `readme.txt` in it, and every file uploaded
// into the folder.
function listing(state: StandIn, folder: string): Wire["Entry"][] {
  const made = state.uploads.filter((one) => one.folder === folder).map((one) => entry(folder, one.name, "file", one.bytes));
  const standing = folder === "" ? [entry("", "docs", "folder", null)] : folder === "docs" ? [entry("docs", "readme.txt", "file", 6)] : [];

  return [...standing, ...made];
}

const OWNER: Wire["Owner"] = { administrator: false, ownerKey: "owner-a", quotaBytes: null, usedBytes: 0 };

const PLACE: Wire["Place"] = {
  name: DRIVE,
  kind: "drive",
  label: DRIVE,
  driveKey: null,
  freeBytes: null,
  mount: null,
  nodeName: null,
  reason: null,
  reasonCode: null
};

type Answer = { status: number; body?: unknown; text?: string };

// What the stand-in answers one call, by its method, its path past the relay's `/drive` and its query.
function answer(state: StandIn, method: string, path: string, query: URLSearchParams, body: Buffer | null): Answer | null {
  const parts = path.split("/").filter((part) => part !== "").map(decodeURIComponent);

  if (method === "GET" && path === "/owner") return { status: 200, body: OWNER };
  if (method === "GET" && path === "/places") return { status: 200, body: { items: [PLACE], more: false } };
  if (method === "GET" && path === "/shared") return { status: 200, body: { items: [], more: false } };

  if (method === "POST" && path === `/places/${DRIVE}/copies`) {
    const asked = JSON.parse(body?.toString() ?? "{}") as { paths: string[]; to: string };
    const items = asked.paths.map((one) => entry(asked.to, `copied ${one.slice(one.lastIndexOf("/") + 1)}`, "file", 6));

    state.copies.push({ paths: asked.paths, to: asked.to });

    return { status: 200, body: { items, more: false, totalCount: items.length } };
  }

  if (parts[0] !== "places" || parts[1] !== DRIVE || parts[2] !== "entries") return null;

  if (method === "GET" && parts.length === 3) {
    const items = listing(state, query.get("path") ?? "");

    return { status: 200, body: { items, more: false, totalCount: items.length } };
  }

  if (method === "GET" && parts.length === 5 && parts[4] === "content") return { status: 200, text: "readme" };

  if (method === "PUT" && parts.length === 5 && parts[4] === "content") {
    const written = parts[3] ?? "";
    const at = written.lastIndexOf("/");

    state.uploads.push({
      folder: at === -1 ? "" : written.slice(0, at),
      name: written.slice(at + 1),
      conflict: query.get("conflict"),
      bytes: body?.length ?? 0
    });

    return { status: 201, body: entry(at === -1 ? "" : written.slice(0, at), written.slice(at + 1), "file", body?.length ?? 0) };
  }

  return null;
}

// The stand-in put behind the screen's calls in a browser context: every fetch the page makes under the relay's
// `/drive`. A page load is the screen's own and goes to the server.
export async function standBehind(context: BrowserContext, state: StandIn): Promise<void> {
  await context.route(
    (url) => url.pathname.startsWith(`/${DRIVE}/`),
    async (route: Route) => {
      const request = route.request();

      if (!["fetch", "xhr", "eventsource"].includes(request.resourceType())) return route.continue();

      const url = new URL(request.url());
      const path = url.pathname.slice(`/${DRIVE}`.length);
      const given = answer(state, request.method(), path, url.searchParams, request.postDataBuffer());

      if (given === null) {
        state.unanswered.push(`${request.method()} ${path}`);

        return route.fulfill({ status: 404, contentType: "application/json", body: JSON.stringify({ code: "not-found", error: "Not in the stand-in." }) });
      }

      if (given.text !== undefined) return route.fulfill({ status: given.status, contentType: "text/plain", body: given.text });

      return route.fulfill({ status: given.status, contentType: "application/json", body: JSON.stringify(given.body ?? null) });
    }
  );
}
