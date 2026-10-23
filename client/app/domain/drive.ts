import { relayAddress } from "bmo-ui/app/domain/relay";
import { joinPath, lastName, parentPath } from "./files";

import type { components } from "../../shared/openapi";
import type { FileConflict, FileDrive, FileDriveKind, FileEntry, FileGarage, FileListing, FileMachine, FilePlaces, FileShareView, FileSortKey, FileSpace, SharedFileView } from "../../shared/types";
import type { FolderAt } from "./files";

// The Files screen's calls to the drive service, each through the console's relay at the path the
// service's own API writes it (docs/api.md), and what each answer becomes in the screen's own view of the
// owner's drive, machines, folders and links. The node list is read from the drive service too, and only
// where it answers that the machines are the signed-in owner's.

type Wire = components["schemas"];

const SERVICE = "drive";

/** The address one of the service's routes is called at, complete as given: the app's base path, the relay and the route. */
export function drivePath(path: string): string {
  return relayAddress(SERVICE, path);
}

// A place's name and an entry's path each stand as one segment of a route's path.
function segment(value: string): string {
  return encodeURIComponent(value);
}

function entryRoute(place: string, path: string): string {
  return `/places/${segment(place)}/entries/${segment(path)}`;
}

// The screen names its columns as its address does; the service names them by the entry's fields.
const SORT_FIELDS: Record<FileSortKey, Wire["EntrySort"]> = { name: "name", size: "byteCount", modified: "changedAt" };

// ---- Machines and places ------------------------------------------------------------------------

/** The key a node's machine is known by in the screen. */
export function nodeMachineKey(nodeName: string): string {
  return `node:${nodeName}`;
}

function drivesOf(machine: Wire["Machine"]): FileDrive[] {
  return machine.drives.map((drive) => ({
    key: drive.key,
    name: drive.name,
    kind: (drive.kind ?? null) as FileDriveKind | null,
    sizeBytes: drive.byteCount ?? null,
    volumes: drive.volumes.map((volume) => ({ place: volume.placeName, mount: volume.mount, fsType: volume.fsType }))
  }));
}

function garageOf(places: readonly Wire["Place"][]): FileGarage | null {
  const store = places.find((one) => one.kind === "store");

  return store === undefined ? null : { place: store.name, disk: store.driveKey ?? null, mount: null, dataDir: null, error: store.reason ?? null, errorCode: store.reasonCode ?? null };
}

function spaceOf(owner: Wire["Owner"], places: readonly Wire["Place"][]): FileSpace | null {
  const drive = places.find((one) => one.kind === "drive");

  return drive === undefined
    ? null
    : { place: drive.name, usedBytes: owner.usedBytes ?? null, quotaBytes: owner.quotaBytes ?? null, error: drive.reason ?? null, errorCode: drive.reasonCode ?? null };
}

/**
 * The signed-in owner's drive, then, where the service answers that the machines are the owner's, this
 * machine read now with its drives and Garage and each node of the node list with its drives unread.
 */
export async function readPlaces(): Promise<FilePlaces> {
  const [owner, places] = await Promise.all([$fetch<Wire["Owner"]>(drivePath("/owner")), $fetch<Wire["PlacePage"]>(drivePath("/places"))]);
  const space = spaceOf(owner, places.items);

  if (!owner.administrator) return { administrator: false, space, machines: [] };

  const [machine, nodes] = await Promise.all([
    $fetch<Wire["Machine"]>(drivePath("/machine")),
    // A node list that cannot be read leaves this machine alone in the list.
    $fetch<Wire["NodePage"]>(drivePath("/nodes")).catch((): Wire["NodePage"] => ({ items: [], more: false }))
  ]);
  const local: FileMachine = { key: "local", name: machine.name, local: true, disks: drivesOf(machine), garage: garageOf(places.items), error: null, errorCode: null };
  const others = nodes.items.map((node): FileMachine => ({ key: nodeMachineKey(node.name), name: node.label, local: false, disks: null, garage: null, error: null, errorCode: null }));

  return { administrator: true, space, machines: [local, ...others] };
}

/** The owner's drive with its usage and quota read again, after a change in it. */
export async function readSpace(space: FileSpace): Promise<FileSpace> {
  const owner = await $fetch<Wire["Owner"]>(drivePath("/owner"));

  return { ...space, usedBytes: owner.usedBytes ?? null, quotaBytes: owner.quotaBytes ?? null };
}

/** One node's machine, read now. */
export async function readNode(machine: FileMachine): Promise<FileMachine> {
  const nodeName = machine.key.slice("node:".length);
  const read = await $fetch<Wire["Machine"]>(drivePath("/machine"), { query: { nodeName } });

  return { ...machine, name: read.name, disks: drivesOf(read), error: null, errorCode: null };
}

// ---- Folders ------------------------------------------------------------------------------------

function entryOf(entry: Wire["Entry"]): FileEntry {
  return { name: entry.name, kind: entry.kind, size: entry.byteCount ?? null, modified: entry.changedAt ?? null };
}

export interface ListingAsked {
  sort?: FileSortKey;
  order?: "asc" | "desc";
  after?: number;
  limit: number;
  folders?: boolean;
  fresh?: boolean;
}

/** One page of a folder, cut from one order over the whole of it. */
export async function readListing(at: FolderAt, asked: ListingAsked): Promise<FileListing> {
  const query: Record<string, string> = { path: at.path, after: String(asked.after ?? 0), limit: String(asked.limit) };

  if (asked.sort !== undefined) query.sort = SORT_FIELDS[asked.sort];
  if (asked.order !== undefined) query.order = asked.order;
  if (asked.folders === true) query.folders = "true";
  if (asked.fresh === true) query.fresh = "true";

  const page = await $fetch<Wire["EntryPage"]>(drivePath(`/places/${segment(at.place)}/entries`), { query });

  return { place: at.place, path: at.path, total: page.totalCount, offset: asked.after ?? 0, entries: page.items.map(entryOf) };
}

/** Every name standing in a folder, read page by page. */
async function namesIn(at: FolderAt): Promise<Set<string>> {
  const names = new Set<string>();
  const limit = 1000;

  for (let after = 0; ; after += limit) {
    const page = await readListing(at, { after, limit, fresh: true });

    page.entries.forEach((entry) => names.add(entry.name));

    if (after + page.entries.length >= page.total || page.entries.length === 0) return names;
  }
}

/** The address a file is read at: shown in the browser, saved, or only its head. */
export function contentUrl(at: FolderAt & { name: string }, how: { download?: boolean; head?: number } = {}): string {
  const query = new URLSearchParams();

  if (how.download === true) query.set("download", "true");
  if (how.head !== undefined) query.set("head", String(how.head));

  const asked = query.toString();

  return drivePath(`${entryRoute(at.place, joinPath(at.path, at.name))}/content${asked === "" ? "" : `?${asked}`}`);
}

// ---- Changes ------------------------------------------------------------------------------------

/** A refusal made where the screen finds the names collide itself, in the shape a refused change answers. */
function conflictRefusal(names: string[]): Error & { statusCode: number; data: { code: string; names: string[] } } {
  return Object.assign(new Error("the names are already taken"), { statusCode: 409, data: { code: "conflict", names } });
}

/** A folder made, settled as `conflict` says where its name is taken: the name it was made under, or null where it was skipped. */
export async function makeFolder(place: string, path: string, conflict: FileConflict): Promise<string | null> {
  const made = await $fetch.raw<Wire["Entry"]>(drivePath(`/places/${segment(place)}/folders`), { method: "POST", body: { path, conflict } });

  return made.status === 200 && conflict === "skip" ? null : (made._data?.name ?? null);
}

/** An entry given a new name in its own folder. */
export async function renameEntry(place: string, path: string, name: string): Promise<void> {
  await $fetch(drivePath(entryRoute(place, path)), { method: "PATCH", body: { path: joinPath(parentPath(path) ?? "", name) } });
}

/** Entries deleted, each a folder with everything in it. */
export async function deleteEntries(place: string, paths: readonly string[]): Promise<void> {
  for (const path of paths) {
    await $fetch(drivePath(entryRoute(place, path)), { method: "DELETE" });
  }
}

/**
 * Entries of one place carried into a folder, copied or moved; the names they arrived under. Inside one
 * place a move is each entry's own move, its collisions read off the folder first so a refusal names them
 * all before anything moves; anything else is the service's copy, and a move between places deletes what
 * was copied once all of it has arrived.
 */
export async function carryEntries(from: { place: string; paths: string[] }, into: FolderAt, move: boolean, conflict: FileConflict): Promise<string[]> {
  if (move && from.place === into.place) {
    const sources = from.paths.filter((path) => (parentPath(path) ?? "") !== into.path);
    const standing = await namesIn(into);
    const colliding = sources.map(lastName).filter((name) => standing.has(name));

    if (conflict === "refuse" && colliding.length > 0) throw conflictRefusal(colliding);

    const landed: string[] = [];

    for (const path of sources) {
      if (conflict === "skip" && standing.has(lastName(path))) continue;

      const moved = await $fetch<Wire["Entry"]>(drivePath(entryRoute(from.place, path)), {
        method: "PATCH",
        body: { path: joinPath(into.path, lastName(path)), conflict: conflict === "skip" ? "refuse" : conflict }
      });

      landed.push(moved.name);
    }

    return landed;
  }

  const copied = await $fetch<Wire["EntryPage"]>(drivePath(`/places/${segment(from.place)}/copies`), {
    method: "POST",
    body: { paths: from.paths, to: into.path, toPlaceName: into.place, conflict }
  });
  const arrived = copied.items.map((entry) => entry.name);

  if (move) {
    // A skipped entry stays where it was; every other arrived, under its own name or a new one.
    const taken = conflict === "skip" ? from.paths.filter((path) => arrived.includes(lastName(path))) : from.paths;

    await deleteEntries(from.place, taken);
  }

  return arrived;
}

/** One file's bytes written into a folder, its progress told as it goes. A refusal is thrown in the shape `$fetch` throws one. */
export function sendFile(into: FolderAt, path: string, file: File, conflict: FileConflict, progress: (sent: number) => void, failure: string): Promise<void> {
  const route = `${entryRoute(into.place, joinPath(into.path, path))}/content?conflict=${conflict}`;

  return new Promise((done, failed) => {
    const request = new XMLHttpRequest();

    request.open("PUT", drivePath(route));
    request.setRequestHeader("content-type", "application/octet-stream");
    request.upload.onprogress = (event) => progress(event.loaded);
    request.onload = () => {
      if (request.status >= 200 && request.status < 300) {
        done();

        return;
      }

      const body = (() => {
        try {
          return JSON.parse(request.responseText) as { code?: unknown; names?: unknown };
        } catch {
          return null;
        }
      })();

      failed({ statusCode: request.status, data: { code: body?.code, names: body?.names } });
    };
    request.onerror = () => failed(new Error(failure));
    request.send(file);
  });
}

// ---- Share links --------------------------------------------------------------------------------

function shareOf(share: Wire["Share"]): FileShareView {
  return {
    token: share.token,
    place: share.placeName,
    path: share.path,
    name: share.name,
    kind: share.kind,
    url: share.url,
    createdBy: share.actorName ?? "",
    createdAt: Math.floor(share.createdAt / 1000),
    expiresAt: share.expiresAt === null || share.expiresAt === undefined ? null : Math.floor(share.expiresAt / 1000)
  };
}

/** Every live link, the newest first. */
export async function readShares(): Promise<FileShareView[]> {
  return (await $fetch<Wire["SharePage"]>(drivePath("/shares"), { query: { limit: "1000" } })).items.map(shareOf);
}

/** The link of one entry, living `seconds` from now or with no end: made now, or the one already standing. */
export async function makeShare(place: string, path: string, seconds: number | null): Promise<FileShareView> {
  const expiresAt = seconds === null ? null : Date.now() + seconds * 1000;

  return shareOf(await $fetch<Wire["Share"]>(drivePath("/shares"), { method: "POST", body: { placeName: place, path, expiresAt } }));
}

/** One link taken away. */
export async function endShare(token: string): Promise<void> {
  await $fetch(drivePath(`/shares/${segment(token)}`), { method: "DELETE" });
}

/** What a link's holder is shown of it. */
export function sharedView(share: Wire["Share"]): SharedFileView {
  const view = shareOf(share);

  return { name: view.name, kind: view.kind, expiresAt: view.expiresAt };
}

/** The route a link's holder reads it at. */
export function shareRoute(token: string): string {
  return drivePath(`/shares/${segment(token)}`);
}

/** The address a link's holder downloads what it hands out from. */
export function shareContentUrl(token: string): string {
  return drivePath(`/shares/${segment(token)}/content`);
}
