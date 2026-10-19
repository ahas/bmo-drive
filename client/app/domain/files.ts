import { fileIcon, folderIcon } from "bmo-ui/app/domain/file-icons";
import { countText } from "bmo-ui/app/domain/numbers";
import { formatStamp } from "bmo-ui/app/domain/time";
import { faultText } from "./refusals";
import { fileSizeText } from "./sizes";
import { appUrl } from "./urls";

import type { FileDrive, FileDriveKind, FileEntry, FileGarage, FileMachine, FilePlaces, FileSortKey, FileVolume } from "../../shared/types";
import type { BarProperty } from "bmo-ui/app/domain/tabs";
import type { Translate } from "bmo-ui/shared/wording";

// The Files screen's own reckoning: where a folder stands, how its list is ordered, paged and laid
// out, what a file can be previewed as, what the clipboard holds, and the tree's roots — the machines,
// each with its drives — out of the places the service reads (docs/files.md).

/** How many entries a page of the list carries; the tree asks for its folders a larger page at once. */
export const PAGE = 200;
export const TREE_PAGE = 1000;

/** How much of a text file the preview reads. */
export const TEXT_HEAD_BYTES = 1024 * 1024;

/** A picture at most this large stands as its own small version where its icon is large enough. */
export const THUMBNAIL_MOST_BYTES = 8 * 1024 * 1024;

// ---- Views --------------------------------------------------------------------------------------

/** The ways the open folder is laid out, as a file explorer offers them. */
export const FILES_VIEWS = ["huge", "large", "medium", "small", "list", "details", "tiles", "content"] as const;

export type FilesView = (typeof FILES_VIEWS)[number];

export const DEFAULT_VIEW: FilesView = "details";

export const VIEW_ICONS: Record<FilesView, string> = {
  huge: "i-material-symbols-photo-size-select-large",
  large: "i-material-symbols-grid-view-outline",
  medium: "i-material-symbols-apps",
  small: "i-material-symbols-view-comfy-outline",
  list: "i-material-symbols-view-column-outline",
  details: "i-material-symbols-view-list-outline",
  tiles: "i-material-symbols-view-module-outline",
  content: "i-material-symbols-view-agenda-outline"
};

/** How large an entry's mark stands in each view, in pixels; a picture shows as itself from 48 up. */
export const VIEW_MARK: Record<FilesView, number> = {
  huge: 192,
  large: 96,
  medium: 48,
  small: 20,
  list: 20,
  details: 20,
  tiles: 48,
  content: 32
};

/** The language pack's name for each view, each key whole so it can be found by reading. */
export const VIEW_LABELS: Record<FilesView, string> = {
  huge: "files.label.view_huge",
  large: "files.label.view_large",
  medium: "files.label.view_medium",
  small: "files.label.view_small",
  list: "files.label.view_list",
  details: "files.label.view_details",
  tiles: "files.label.view_tiles",
  content: "files.label.view_content"
};

/** The language pack's name for each column a folder is sorted by. */
export const SORT_LABELS: Record<FileSortKey, string> = {
  name: "files.label.name",
  modified: "files.label.modified",
  size: "files.label.size"
};

export function readView(value: unknown): FilesView {
  return (FILES_VIEWS as readonly unknown[]).includes(value) ? (value as FilesView) : DEFAULT_VIEW;
}

export interface FilesSort {
  by: FileSortKey;
  order: "asc" | "desc";
}

export const DEFAULT_SORT: FilesSort = { by: "name", order: "asc" };

// ---- Paths ----------------------------------------------------------------------------------------

/** A folder of one place: the place and the path under it, the empty path being its top. */
export interface FolderAt {
  place: string;
  path: string;
}

export function joinPath(path: string, name: string): string {
  return path === "" ? name : `${path}/${name}`;
}

/** The folder above a path, or null at the place's top. */
export function parentPath(path: string): string | null {
  if (path === "") {
    return null;
  }

  const cut = path.lastIndexOf("/");

  return cut < 0 ? "" : path.slice(0, cut);
}

export function lastName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** The path bar's steps: the place, then each folder down to the open one. */
export function crumbs(placeLabel: string, path: string): { label: string; path: string }[] {
  const names = path === "" ? [] : path.split("/");

  return [{ label: placeLabel, path: "" }, ...names.map((name, at) => ({ label: name, path: names.slice(0, at + 1).join("/") }))];
}

/** The order after a column's name is pressed: the same column turns round, another starts ascending. */
export function nextSort(sort: FilesSort, by: FileSortKey): FilesSort {
  return sort.by === by ? { by, order: sort.order === "asc" ? "desc" : "asc" } : { by, order: "asc" };
}

export function readSort(by: unknown, order: unknown): FilesSort {
  const key: FileSortKey = by === "size" || by === "modified" ? by : "name";

  return { by: key, order: order === "desc" ? "desc" : "asc" };
}

// ---- What a file is -------------------------------------------------------------------------------

const IMAGE_ENDS = new Set(["png", "jpg", "jpeg", "gif", "webp", "bmp", "avif", "ico", "svg"]);

// Endings tried in the browser's own player; one whose codec it cannot play says so when it is tried.
const VIDEO_ENDS = new Set(["mp4", "m4v", "webm", "mov", "mkv", "ogv", "avi", "3gp", "wmv", "flv", "mpg", "mpeg"]);

// Endings that are never text, so no head is asked of them.
const BINARY_ENDS = new Set([
  "zip", "gz", "tgz", "xz", "zst", "bz2", "7z", "rar", "tar", "iso", "img", "qcow2", "vmdk",
  "mp3", "wav", "ogg", "flac", "m4a", "aac", "opus",
  "exe", "dll", "so", "o", "a", "bin", "class", "jar", "wasm", "pyc",
  "db", "sqlite", "sqlite3", "lmdb", "mdb", "parquet", "arrow", "npy", "pt", "safetensors", "gguf", "onnx",
  "ttf", "otf", "woff", "woff2", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "hwp", "psd"
]);

export type PreviewKind = "image" | "pdf" | "video" | "text" | "none";

function endOf(name: string): string {
  const dot = name.lastIndexOf(".");

  return dot <= 0 ? "" : name.slice(dot + 1).toLowerCase();
}

/**
 * What a file is previewed as, by its name: a picture, a PDF, a video, or text — every name not known
 * to be something else is tried as text, and the head read decides (`textOf`).
 */
export function previewKind(name: string): PreviewKind {
  const end = endOf(name);

  if (IMAGE_ENDS.has(end)) return "image";
  if (end === "pdf") return "pdf";
  if (VIDEO_ENDS.has(end)) return "video";
  if (BINARY_ENDS.has(end)) return "none";

  return "text";
}

export function isJson(name: string): boolean {
  return ["json", "jsonc", "jsonl", "geojson", "webmanifest"].includes(endOf(name));
}

/** A head read as text, or null where it is not text: bytes that are not UTF-8, or a NUL among them. */
export function textOf(bytes: Uint8Array, cut: boolean): string | null {
  if (bytes.includes(0)) {
    return null;
  }

  try {
    // A head cut mid-character ends in a broken sequence; only the last few bytes may be one.
    return new TextDecoder("utf-8", { fatal: true }).decode(cut ? trimmedTail(bytes) : bytes);
  } catch {
    return null;
  }
}

// The bytes up to the start of a character the cut broke in two.
function trimmedTail(bytes: Uint8Array): Uint8Array {
  for (let back = 1; back <= Math.min(3, bytes.length); back++) {
    const byte = bytes[bytes.length - back]!;

    if ((byte & 0xc0) !== 0x80) {
      const width = byte >= 0xf0 ? 4 : byte >= 0xe0 ? 3 : byte >= 0xc0 ? 2 : 1;

      return width > back ? bytes.subarray(0, bytes.length - back) : bytes;
    }
  }

  return bytes;
}

/** A JSON file laid out with its indentation; text that is not JSON stands as it is. */
export function laidOutJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

/** A file's size as a file explorer's list writes it: in kilobytes, rounded up, with separators. */
export function listSize(bytes: number): string {
  return `${countText(Math.ceil(bytes / 1024))}KB`;
}

/** The type a file explorer names an entry by: a folder, a file of its ending, or a plain file. */
export function typeLabel(t: Translate, entry: FileEntry): string {
  if (entry.kind === "folder") return t("files.label.folder");

  const dot = entry.name.lastIndexOf(".");

  return dot > 0 ? t("files.label.file_type", { end: entry.name.slice(dot + 1).toUpperCase() }) : t("files.label.plain_file");
}

/** An entry's details as the properties viewer lists them: where it is, what it is, how large, when changed. */
export function entryProperties(t: Translate, placeName: string, path: string, entry: FileEntry): BarProperty[] {
  return [
    { label: t("files.label.place"), value: placeName },
    { label: t("files.label.path"), value: path, mono: true },
    { label: t("files.label.type"), value: typeLabel(t, entry) },
    ...(entry.size === null ? [] : [{ label: t("files.label.size"), value: t("files.label.size_exact", { size: fileSizeText(entry.size), bytes: countText(entry.size) }) }]),
    ...(entry.modified === null ? [] : [{ label: t("files.label.modified"), value: formatStamp(entry.modified / 1000) }])
  ];
}

/** The address a file is read at: shown in the browser, saved, or only its head. */
export function readUrl(at: FolderAt & { name: string }, how: { download?: boolean; head?: number } = {}): string {
  const query = new URLSearchParams({ place: at.place, path: joinPath(at.path, at.name) });

  if (how.download === true) query.set("download", "1");
  if (how.head !== undefined) query.set("head", String(how.head));

  return appUrl(`/api/files/read?${query.toString()}`);
}

/** Whether an entry stands as its own small picture at a mark of this size. */
export function showsThumbnail(entry: FileEntry, mark: number): boolean {
  return mark >= 48 && entry.kind === "file" && previewKind(entry.name) === "image" && (entry.size ?? Infinity) <= THUMBNAIL_MOST_BYTES;
}

export function entryIcon(entry: FileEntry, open = false): string {
  return entry.kind === "folder" ? folderIcon(entry.name, open) : entry.kind === "file" ? fileIcon(entry.name) : "i-material-symbols-draft-outline";
}

// ---- Machines and places --------------------------------------------------------------------------

const DRIVE_ICONS: Record<FileDriveKind, string> = {
  nvme: "i-material-symbols-hard-drive-2-outline",
  ssd: "i-material-symbols-hard-drive-2-outline",
  hdd: "i-material-symbols-hard-disk",
  usb: "i-material-symbols-usb",
  network: "i-material-symbols-cloud-outline",
  virtual: "i-material-symbols-storage"
};

export const GARAGE_ICON = "i-material-symbols-inventory-2-outline";

export function machineIcon(machine: FileMachine): string {
  return machine.local ? "i-material-symbols-computer-outline" : "i-material-symbols-dns-outline";
}

export function driveIcon(drive: FileDrive): string {
  return drive.kind === null ? "i-material-symbols-hard-drive-outline" : DRIVE_ICONS[drive.kind];
}

// How the service names a node's machine and every place on it: the node's name after a prefix, and a
// place's mount after that. Every other place is this machine's.
const NODE_PREFIX = "node:";

/** The key of the machine a place stands on: its node's, read off the place's own name, or this one's. */
export function machineKeyOf(place: string): string {
  if (!place.startsWith(NODE_PREFIX)) return "local";

  const rest = place.slice(NODE_PREFIX.length);
  const cut = rest.indexOf(":");

  return `${NODE_PREFIX}${cut < 0 ? rest : rest.slice(0, cut)}`;
}

/** Where a place stands among the machines read so far: its machine, and its drive and volume. */
export interface PlaceFound {
  machine: FileMachine;
  drive: FileDrive | null;
  volume: FileVolume | null;
}

export function findPlace(places: FilePlaces | null, place: string): PlaceFound | null {
  for (const machine of places?.machines ?? []) {
    if (machine.garage?.place === place) return { machine, drive: null, volume: null };

    for (const drive of machine.disks ?? []) {
      const volume = drive.volumes.find((one) => one.place === place);

      if (volume !== undefined) return { machine, drive, volume };
    }
  }

  return null;
}

/**
 * A place's name where it stands at the head of the path bar: its drive's name, and the mount point
 * where the drive has several, after its machine's name where that is a node. Before the places are
 * read it is the mount point its own name carries.
 */
export function placeLabel(t: Translate, places: FilePlaces | null, place: string): string {
  const found = findPlace(places, place);

  if (found === null) {
    const cut = place.lastIndexOf(":");

    return place === "garage" ? t("files.label.garage") : cut < 0 ? place : place.slice(cut + 1) || place;
  }

  const { machine, drive, volume } = found;
  const own = drive === null || volume === null ? t("files.label.garage") : drive.volumes.length > 1 ? `${drive.name} ${volume.mount}` : drive.name;

  return machine.local ? own : `${machine.name} · ${own}`;
}

/** Where Garage is installed, as its row says it: the drive's name and the mount point. */
export function garageWhere(machine: FileMachine, garage: FileGarage): string {
  const drive = machine.disks?.find((one) => one.key === garage.disk);

  return [drive?.name, garage.mount].filter((one): one is string => one !== null && one !== undefined).join(" · ");
}

/** The first place to open where the address names none: this machine's root volume, else its first. */
export function firstPlace(places: FilePlaces): string | null {
  const here = places.machines.find((one) => one.local);
  const volumes = (here?.disks ?? []).flatMap((drive) => drive.volumes);

  return (volumes.find((one) => one.mount === "/") ?? volumes[0])?.place ?? here?.garage?.place ?? null;
}

/** The machines with one of them replaced by a fresh reading of it. */
export function withMachine(places: FilePlaces, read: FileMachine): FilePlaces {
  return { machines: places.machines.map((one) => (one.key === read.key ? read : one)) };
}

// ---- The tree -----------------------------------------------------------------------------------

/** One row of the folder tree, as Nuxt UI's tree takes it. */
export interface FilesTreeItem {
  // `machine:<key>` for a machine, `drive:<machine>\t<drive>` for a drive with several volumes,
  // `<place>\n<path>` for a folder of a place.
  value: string;
  label: string;
  // What stands after the name: a mount point, where Garage stands, why it cannot be opened.
  note?: string;
  failed?: boolean;
  icon: string;
  // The folder the row opens, where it opens one.
  folder?: FolderAt;
  // The machine the row is, where it is one.
  machine?: string;
  disabled?: boolean;
  // A row standing in for what is still being read.
  loading?: boolean;
  children?: FilesTreeItem[];
}

export function folderKey(at: FolderAt): string {
  return `${at.place}\n${at.path}`;
}

export function machineRowKey(key: string): string {
  return `machine:${key}`;
}

function driveRowKey(machine: string, drive: string): string {
  return `drive:${machine}\t${drive}`;
}

/** What the tree knows of one folder's own folders. */
export type FolderChildren = { state: "loading" } | { state: "failed"; reason: string } | { state: "read"; names: string[]; more: number };

// What every row of the tree is drawn with: what is known of each folder, and how a row of folders
// past the tree's page is said.
interface TreeContext {
  known: ReadonlyMap<string, FolderChildren>;
  more: (count: number) => string;
}

function loadingRow(key: string): FilesTreeItem {
  return { value: `${key}\n…`, label: "", icon: "", loading: true, disabled: true };
}

function failedRow(key: string, reason: string): FilesTreeItem {
  return { value: `${key}\n!`, label: reason, icon: "i-material-symbols-error-outline", failed: true, disabled: true };
}

function folderItem(at: FolderAt, label: string, icon: string, context: TreeContext, note?: string): FilesTreeItem {
  const key = folderKey(at);
  const children = context.known.get(key);
  const item: FilesTreeItem = { value: key, label, icon, folder: at, ...(note === undefined ? {} : { note }) };

  if (children === undefined || children.state === "loading") {
    // A folder not read yet still opens: its row carries the place its folders will stand in.
    return { ...item, children: [loadingRow(key)] };
  }

  if (children.state === "failed") {
    return { ...item, children: [failedRow(key, children.reason)] };
  }

  const inner = children.names.map((name) => folderItem({ place: at.place, path: joinPath(at.path, name) }, name, "", context));

  if (children.more > 0) {
    inner.push({ value: `${key}\n+`, label: context.more(children.more), icon: "i-material-symbols-more-horiz", disabled: true });
  }

  return inner.length === 0 ? item : { ...item, children: inner };
}

function driveItems(t: Translate, machine: FileMachine, drives: readonly FileDrive[], context: TreeContext): FilesTreeItem[] {
  const items = drives.map((drive): FilesTreeItem => {
    const icon = driveIcon(drive);
    const [only] = drive.volumes;

    if (drive.volumes.length === 0) {
      return { value: driveRowKey(machine.key, drive.key), label: drive.name, note: t("files.label.not_mounted"), icon, disabled: true };
    }

    if (drive.volumes.length === 1 && only !== undefined) {
      return folderItem({ place: only.place, path: "" }, drive.name, icon, context, only.mount);
    }

    return {
      value: driveRowKey(machine.key, drive.key),
      label: drive.name,
      icon,
      children: drive.volumes.map((volume) => folderItem({ place: volume.place, path: "" }, volume.mount, icon, context))
    };
  });

  const garage = machine.garage;

  if (garage === null) {
    return items;
  }

  const where = garageWhere(machine, garage);

  if (garage.error !== null) {
    return [...items, { value: folderKey({ place: garage.place, path: "" }), label: t("files.label.garage"), note: faultText(t, garage.errorCode, garage.error), failed: true, icon: GARAGE_ICON, disabled: true }];
  }

  return [...items, folderItem({ place: garage.place, path: "" }, t("files.label.garage"), GARAGE_ICON, context, where === "" ? undefined : where)];
}

/** The tree: each machine — this one first — opening onto its drives and their volumes, and Garage. */
export function filesTree(t: Translate, places: FilePlaces, known: ReadonlyMap<string, FolderChildren>): FilesTreeItem[] {
  const context: TreeContext = { known, more: (count) => t("files.label.more_folders", { n: countText(count) }) };

  return places.machines.map((machine): FilesTreeItem => {
    const key = machineRowKey(machine.key);
    const item: FilesTreeItem = { value: key, label: machine.name, icon: machineIcon(machine), machine: machine.key };

    if (machine.disks === null) {
      // A machine not read yet still opens: its drives are read as it is unfolded.
      return { ...item, children: [machine.error === null ? loadingRow(key) : failedRow(key, faultText(t, machine.errorCode ?? null, machine.error))] };
    }

    const drives = driveItems(t, machine, machine.disks, context);

    return drives.length === 0 ? { ...item, note: t("files.label.no_drives") } : { ...item, children: drives };
  });
}

/** Every row key from a machine down to a folder, which the tree unfolds to show it. */
export function keysTo(places: FilePlaces, at: FolderAt): string[] {
  const found = findPlace(places, at.place);
  const machine = found?.machine.key ?? machineKeyOf(at.place);
  const names = at.path === "" ? [] : at.path.split("/");
  const own = names.slice(0, -1).map((_, depth) => folderKey({ place: at.place, path: names.slice(0, depth + 1).join("/") }));
  const drive = found?.drive;

  return [
    machineRowKey(machine),
    ...(drive !== null && drive !== undefined && drive.volumes.length > 1 ? [driveRowKey(machine, drive.key)] : []),
    ...(at.path === "" ? [] : [folderKey({ place: at.place, path: "" })]),
    ...own
  ];
}

// ---- The clipboard --------------------------------------------------------------------------------

/** What was cut or copied: entries of one place, and whether pasting moves them. */
export interface FilesClip {
  mode: "copy" | "cut";
  place: string;
  paths: string[];
}

/** Whether an entry is waiting, cut, to be moved — drawn dimmed until it is pasted. */
export function isCut(clip: FilesClip | null, at: FolderAt, name: string): boolean {
  return clip !== null && clip.mode === "cut" && clip.place === at.place && clip.paths.includes(joinPath(at.path, name));
}

/** Whether pasting the clip into a folder is a copy into the folder its entries came from. */
export function pastesBeside(clip: FilesClip, into: FolderAt): boolean {
  return clip.place === into.place && clip.paths.every((path) => (parentPath(path) ?? "") === into.path);
}

/** The path a place's entry is copied out as: the machine and mount it stands on, then the path. */
export function shownPath(places: FilePlaces | null, place: string, path: string): string {
  const found = findPlace(places, place);
  const mount = found?.volume?.mount ?? "";
  const windows = /^[A-Za-z]:$/.test(mount);
  const joined = windows ? `${mount}\\${path.replaceAll("/", "\\")}` : mount === "" ? `/${path}` : `${mount.replace(/\/$/, "")}/${path}`;

  return found === null || found.machine.local ? joined : `${found.machine.name}:${joined}`;
}

// ---- Dropped files --------------------------------------------------------------------------------

/** One thing dropped from the desktop: a folder to make, or a file to upload, by its path inside the drop. */
export type Dropped = { kind: "folder"; path: string } | { kind: "file"; path: string; file: File };

// A dropped folder's entries, read the way the browser hands them: in batches until one comes empty.
function entriesOf(folder: FileSystemDirectoryEntry): Promise<FileSystemEntry[]> {
  const reader = folder.createReader();
  const all: FileSystemEntry[] = [];

  return new Promise((done, fail) => {
    const next = (): void =>
      reader.readEntries((batch) => {
        if (batch.length === 0) {
          done(all);
        } else {
          all.push(...batch);
          next();
        }
      }, fail);

    next();
  });
}

async function walkEntry(entry: FileSystemEntry, path: string, into: Dropped[]): Promise<void> {
  if (entry.isDirectory) {
    into.push({ kind: "folder", path });

    for (const inner of await entriesOf(entry as FileSystemDirectoryEntry)) {
      await walkEntry(inner, joinPath(path, inner.name), into);
    }
  } else if (entry.isFile) {
    const file = await new Promise<File>((done, fail) => (entry as FileSystemFileEntry).file(done, fail));

    into.push({ kind: "file", path, file });
  }
}

/** Everything a drop carries, each folder before what is in it, so making them in order makes the tree. */
export async function droppedItems(transfer: DataTransfer): Promise<Dropped[]> {
  const entries = [...transfer.items].map((item) => item.webkitGetAsEntry()).filter((one): one is FileSystemEntry => one !== null);

  if (entries.length === 0) {
    return [...transfer.files].map((file) => ({ kind: "file", path: file.name, file }));
  }

  const dropped: Dropped[] = [];

  for (const entry of entries) {
    await walkEntry(entry, entry.name, dropped);
  }

  return dropped;
}

/** Whether a drag carries files from outside the page rather than entries of the list. */
export function carriesFiles(transfer: DataTransfer | null): boolean {
  return transfer !== null && [...transfer.types].includes("Files");
}

/** The drag type entries of the list travel under, inside the page. */
export const ENTRIES_DRAG = "application/x-bmo-files";
