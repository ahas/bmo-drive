// The file viewer's places and folders as the service answers them (docs/api.md, "The file viewer").
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

export interface FilePlaces {
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
  // Unix milliseconds.
  modified: number | null;
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

// One live share link of an entry, as the administrator's own screen reads it. The token is the link
// itself, so this shape goes to a logged-in screen and nowhere else.
export interface FileShareView {
  token: string;
  place: string;
  path: string;
  // The name the entry is handed out under: its own, and at a place's top the place's last name.
  name: string;
  kind: FileShareKind;
  createdBy: string;
  // Unix seconds.
  createdAt: number;
  // When the link stops opening, and null where it never does.
  expiresAt: number | null;
}

// What the holder of a link is shown: the entry's name and kind, a file's size, and when the link
// ends. The place and the path stay on the service.
export interface SharedFileView {
  name: string;
  kind: FileShareKind;
  size: number | null;
  expiresAt: number | null;
}
