// The Files screen's own view of what the service answers (docs/api.md): the signed-in owner's drive, the
// machines with their drives and places, a folder's entries and the share links, as the screen draws them. The service's own shapes
// are the generated `openapi.ts`; the screen's calls turn each answer into these (app/domain/drive.ts).
export type FileDriveKind = "nvme" | "ssd" | "hdd" | "usb" | "network" | "virtual";

export interface FileVolume {
  // The place the viewer walks it by.
  place: string;
  mount: string;
  fsType: string;
}

export interface FileDrive {
  key: string;
  // The drive's model.
  name: string;
  kind: FileDriveKind | null;
  sizeBytes: number | null;
  volumes: FileVolume[];
}

export interface FileGarage {
  place: string;
  // The drive key and mount point its data stands on.
  disk: string | null;
  mount: string | null;
  dataDir: string | null;
  // Why it cannot be opened, where it cannot: an English sentence and the code the screen words.
  error: string | null;
  errorCode: string | null;
}

// One machine the viewer walks: this one, or a node of the node list. A node's drives are null
// until they are read, and where they could not be read the error says why.
export interface FileMachine {
  // `local` for this machine, `node:<name>` for a node.
  key: string;
  // This machine's host name, or the node's label.
  name: string;
  local: boolean;
  disks: FileDrive[] | null;
  garage: FileGarage | null;
  error: string | null;
  errorCode?: string | null;
}

// The signed-in owner's BMO drive (docs/files.md, "The BMO drive"): its place, the bytes its files hold and
// its quota, and why it cannot be opened, where it cannot.
export interface FileSpace {
  place: string;
  // Null where the storage cannot be opened.
  usedBytes: number | null;
  // Null where the owner has no quota.
  quotaBytes: number | null;
  error: string | null;
  errorCode: string | null;
}

export interface FilePlaces {
  // Whether the machines are the signed-in owner's, as the service answers it; none are read where not.
  administrator: boolean;
  // The signed-in owner's key, as the service answers it: what the screen remembers in the browser is kept
  // under it, so each viewer has their own.
  viewer: string;
  space: FileSpace | null;
  machines: FileMachine[];
}

// How a name already taken in the folder a change lands in is settled (docs/files.md, "Changing
// files").
export type FileConflict = "refuse" | "replace" | "rename" | "skip";

// A refused change's body where the names collided.
export interface FileConflictRefusal {
  error: string;
  code: string;
  names: string[];
}

export type FileEntryKind = "folder" | "file" | "other";

export interface FileEntry {
  name: string;
  kind: FileEntryKind;
  size: number | null;
  // Epoch milliseconds.
  modified: number | null;
  // For a link, its target as the link writes it; null for anything else.
  link: string | null;
}

// One page of a folder, and how many entries the whole folder holds.
export interface FileListing {
  place: string;
  path: string;
  total: number;
  offset: number;
  entries: FileEntry[];
}

export type FileSortKey = "name" | "size" | "modified";

// What a share link hands out: one file, or one folder as its zip (docs/files.md, "Share links").
export type FileShareKind = "file" | "folder";

// One live share link of an entry. The token is the link itself, so this shape goes to a screen that
// holds the service's token and nowhere else.
export interface FileShareView {
  token: string;
  place: string;
  path: string;
  // The name the entry is handed out under: its own, and at a place's top the place's last name.
  name: string;
  kind: FileShareKind;
  // The address its page stands at.
  url: string;
  createdBy: string;
  // Unix seconds, as the share link dialog reads a time.
  createdAt: number;
  // When the link stops opening, in Unix seconds, and null where it never does.
  expiresAt: number | null;
}

// What the holder of a link is shown: the entry's name and kind, and when the link ends.
export interface SharedFileView {
  name: string;
  kind: FileShareKind;
  expiresAt: number | null;
}
