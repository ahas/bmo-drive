import { isSharedPlace, sharedOf } from "./files";

import type { FilePlaces } from "../../shared/types";
import type { FolderAt } from "./files";

// Where the Files screen opens (docs/files.md, "The Files screen"). The address names the place and the
// folder, so a reload and the browser's history keep them; where the address names none, the screen opens
// at the folder the viewer last opened, remembered in this browser under the viewer's owner key as a
// convenience. Storage the browser refuses, or has cleared, remembers nothing and changes nothing else.

/** The part of the browser's storage the memory uses, so another store stands in where there is none. */
export interface PlaceStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

const PREFIX = "bmo-drive.files.place:";

function storeKey(viewer: string): string {
  return `${PREFIX}${viewer}`;
}

/** This browser's own storage, or none where the browser refuses it. */
export function browserStore(): PlaceStore | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

/** The folder the viewer last opened, where one is remembered and reads as one. */
export function rememberedPlace(store: PlaceStore | null, viewer: string): FolderAt | null {
  if (store === null || viewer === "") return null;

  try {
    const kept: unknown = JSON.parse(store.getItem(storeKey(viewer)) ?? "null");
    const { place, path } = (kept ?? {}) as { place?: unknown; path?: unknown };

    return typeof place === "string" && place !== "" && typeof path === "string" ? { place, path } : null;
  } catch {
    return null;
  }
}

/** The folder the viewer opens, remembered as their last. */
export function rememberPlace(store: PlaceStore | null, viewer: string, at: FolderAt): void {
  if (store === null || viewer === "") return;

  try {
    store.setItem(storeKey(viewer), JSON.stringify({ place: at.place, path: at.path }));
  } catch {
    // Nothing is remembered where the browser refuses it.
  }
}

/** The viewer's remembered folder let go, as one that can no longer be listed is. */
export function forgetPlace(store: PlaceStore | null, viewer: string): void {
  if (store === null || viewer === "") return;

  try {
    store.removeItem(storeKey(viewer));
  } catch {
    // What the browser refuses to take away is read again and let go again.
  }
}

/**
 * Whether the viewer still reaches a place: their own drive, a share still shared with them, or any other
 * place where the machines are theirs.
 */
function reaches(places: FilePlaces, place: string): boolean {
  return place === places.space?.place || sharedOf(places, place) !== null || (places.administrator && !isSharedPlace(place));
}

/**
 * Where the screen opens: the folder the address names; else the remembered one, where its place is still
 * the viewer's; else the top of `first`, the place the screen opens with nothing remembered.
 */
export function startingPlace(named: FolderAt | null, remembered: FolderAt | null, places: FilePlaces, first: string | null): FolderAt | null {
  if (named !== null) return named;
  if (remembered !== null && reaches(places, remembered.place)) return remembered;

  return first === null ? null : { place: first, path: "" };
}
