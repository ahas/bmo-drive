//! The service's wire: every body and query a route takes or answers, in the common wire format
//! (`docs/api.md`, 「Conventions」). The routes answer with these types, the client sends and reads them, and
//! the OpenAPI document is generated from their schemas, so the three cannot drift apart.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Whether the service is ready, why not where it is not, and its version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Health {
  /// The store is open and migrated.
  pub ready: bool,
  /// Why it is not ready; null while it is.
  pub reason: Option<String>,
  pub version: String,
}

/// A failure, in the common error shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WireError {
  /// The kebab-case code a caller matches on.
  pub code: String,
  /// The English sentence, for a log.
  pub error: String,
  /// Every entry that would collide, beside a `conflict` over a name already taken.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub names: Option<Vec<String>>,
}

/// One event of the service's stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Event {
  /// The event number, which only grows.
  pub id: i64,
  /// The resource and its change: `entry.created`, `share.deleted`.
  pub kind: String,
  /// When it happened, in epoch milliseconds.
  pub at: i64,
  /// The member as a read answers it, or its key alone for a deletion.
  pub data: Value,
}

/// A page of the kept events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventPage {
  pub items: Vec<Event>,
  pub more: bool,
}

/// What kind of root a place is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlaceKind {
  /// The asking owner's BMO drive.
  Drive,
  /// A volume of this machine.
  Volume,
  /// Garage, the object storage this machine runs.
  Store,
  /// A volume of a node.
  Node,
}

/// One root files are walked under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Place {
  /// Its key: `drive`, `volume:<mount>`, `garage` or `node:<nodeName>:<mount>`.
  pub name: String,
  pub kind: PlaceKind,
  /// What a person reads it as.
  pub label: String,
  /// The bytes free on a volume of this machine, or left in the drive; null for Garage and a node's volume.
  pub free_bytes: Option<u64>,
  /// The node it stands on, null for this machine.
  pub node_name: Option<String>,
  /// The drive it stands on, where it stands on a listed one.
  pub drive_key: Option<String>,
  /// Where it is mounted, null for Garage.
  pub mount: Option<String>,
  /// Why it cannot be opened, an English sentence; null where it opens.
  pub reason: Option<String>,
  /// The code of that reason, null where it opens.
  pub reason_code: Option<String>,
}

/// What the asking owner may reach (`docs/api.md`, 「Owners」).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
  /// The asking owner's key, as the request gave it.
  pub owner_key: String,
  /// Whether the machine branch is this owner's.
  pub administrator: bool,
  /// The bytes of the files in its space; null where the storage cannot be opened.
  pub used_bytes: Option<u64>,
  /// Its quota; null for the administrator, who has none.
  pub quota_bytes: Option<u64>,
}

/// What kind of location the BMO drive stands in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum StorageKind {
  /// A root folder on this machine.
  Local,
  /// A root folder on a node of the node list.
  Node,
  /// A key prefix in a bucket of an S3 store.
  S3,
}

/// Where the BMO drive stands and how it is kept, as the settings state it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Storage {
  pub kind: StorageKind,
  /// The root folder, `<node>:<root>`, or `s3://<bucket>/<root>`.
  pub location: String,
  /// How it is backed up, as the settings say it; null where they say nothing.
  pub backup: Option<String>,
  /// The quota of a space no quota was set for.
  pub default_quota_bytes: u64,
  /// Why it cannot be opened, an English sentence; null where it opens.
  pub reason: Option<String>,
  /// The code of that reason, null where it opens.
  pub reason_code: Option<String>,
}

/// One owner's quota and usage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
  pub owner_key: String,
  /// The bytes its files may hold together; null for the administrator's key.
  pub quota_bytes: Option<u64>,
  /// The bytes of the files in its space; null where the storage cannot be opened.
  pub used_bytes: Option<u64>,
  /// Whether no quota was set for the key and the default stands.
  pub defaulted: bool,
}

/// A quota to be set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuotaRequest {
  /// The bytes the owner's files may hold together; null or absent for the default.
  #[serde(default)]
  pub quota_bytes: Option<u64>,
}

/// A page of places.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlacePage {
  pub items: Vec<Place>,
  pub more: bool,
}

/// A node of the node list, as the administrator reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListedNode {
  /// Its name on the node list, by which a place or a machine asks for it.
  pub name: String,
  /// What a person reads it as: its label, or its name where the list gives it none.
  pub label: String,
}

/// A page of the node list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodePage {
  pub items: Vec<ListedNode>,
  pub more: bool,
}

/// A machine and its drives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
  /// This machine's host name, or the node's label.
  pub name: String,
  /// The node, null for this machine.
  pub node_name: Option<String>,
  pub drives: Vec<Drive>,
}

/// One drive of a machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
  /// The kernel's name, or `net:<source>` for a network share.
  pub key: String,
  /// Its model, or its kernel name, or the share's source.
  pub name: String,
  /// `nvme`, `ssd`, `hdd`, `usb`, `network` or `virtual`, where it is known.
  pub kind: Option<String>,
  /// Its size in bytes, where it is known.
  pub byte_count: Option<u64>,
  pub volumes: Vec<DriveVolume>,
}

/// A mounted volume of a drive, each a place of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DriveVolume {
  pub place_name: String,
  pub mount: String,
  pub fs_type: String,
}

/// What an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
  Folder,
  File,
  /// A device, a socket, a pipe, or a link that leads nowhere.
  Other,
}

/// One entry of a place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
  pub place_name: String,
  /// Its names from the place's top, joined by `/`.
  pub path: String,
  /// The path's last name, empty at a place's top.
  pub name: String,
  pub kind: EntryKind,
  /// A file's length in bytes; null for anything else.
  pub byte_count: Option<u64>,
  /// When it last changed, in epoch milliseconds, or null where the place keeps no time.
  pub changed_at: Option<i64>,
  /// For a symbolic link or a junction, its target as the link writes it; null for anything else.
  pub link_target: Option<String>,
}

/// An entry's key alone, as an event of its deletion carries it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntryKey {
  pub place_name: String,
  pub path: String,
}

/// A page of entries, and how many the whole asking holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntryPage {
  pub items: Vec<Entry>,
  pub more: bool,
  pub total_count: u64,
}

/// How a name already taken where an entry lands is settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Conflict {
  /// Nothing is changed, and the colliding names are said.
  Refuse,
  /// The entry standing there gives way.
  Replace,
  /// The arriving entry takes the first free name with a number added.
  Rename,
  /// The colliding entries are left out.
  Skip,
}

/// The column a folder is ordered by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum EntrySort {
  #[default]
  Name,
  ByteCount,
  ChangedAt,
}

/// Which way an order runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
  #[default]
  Asc,
  Desc,
}

/// What a share link hands out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ShareKind {
  File,
  Folder,
}

/// One live share link. It carries its token, which is the whole of the link.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Share {
  pub token: String,
  pub place_name: String,
  pub path: String,
  /// The name its entry is handed out under.
  pub name: String,
  pub kind: ShareKind,
  /// The address its page stands at.
  pub url: String,
  /// Who made it, where its maker named someone.
  pub actor_name: Option<String>,
  /// When it was made, in epoch milliseconds.
  pub created_at: i64,
  /// When it stops opening, in epoch milliseconds, or null for no end.
  pub expires_at: Option<i64>,
}

/// A share link's key alone, as an event of its deletion carries it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShareKey {
  pub token: String,
}

/// A page of share links.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharePage {
  pub items: Vec<Share>,
  pub more: bool,
}

/// An entry moved to another path of its own place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntryMove {
  /// Where it is to stand.
  pub path: String,
  /// How a name already taken there is settled; refused where none is asked.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub conflict: Option<Conflict>,
}

/// A folder to be made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FolderRequest {
  pub path: String,
  /// How a name already taken there is settled; refused where none is asked.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub conflict: Option<Conflict>,
}

/// Entries to be copied into a folder of this place or another.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CopyRequest {
  pub paths: Vec<String>,
  /// The folder they arrive in.
  pub to: String,
  /// The place of that folder, this same one where none is named.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub to_place_name: Option<String>,
  /// How a name already taken there is settled; refused where none is asked.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub conflict: Option<Conflict>,
}

/// Entries to be read as one zip.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveRequest {
  pub paths: Vec<String>,
}

/// A share link asked for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShareRequest {
  pub place_name: String,
  pub path: String,
  /// When it stops opening, in epoch milliseconds; null or absent for no end.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub expires_at: Option<i64>,
  /// Who makes it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub actor_name: Option<String>,
}

/// What a member share lets its member do (`docs/files.md`, 「What a member share is」): the service keeps and
/// serves `read` alone, and refuses `edit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SharePermission {
  Read,
  Edit,
}

/// One member an entry is shared with, as its owner is told of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberShare {
  /// Its number.
  pub id: i64,
  /// The member's owner key.
  pub user: String,
  pub permission: SharePermission,
  /// When it was made, in epoch milliseconds.
  pub shared_at: i64,
}

/// A page of member shares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberSharePage {
  pub items: Vec<MemberShare>,
  pub more: bool,
}

/// An entry asked to be shared with members.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberShareRequest {
  pub place_name: String,
  pub path: String,
  /// The members' owner keys.
  pub users: Vec<String>,
  pub permission: SharePermission,
}

/// A member share's permission asked to change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberShareChange {
  pub permission: SharePermission,
}

/// The query of an entry's member shares.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberSharesQuery {
  pub place_name: String,
  pub path: String,
  /// An `id`: the member shares after it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u64>,
}

/// One entry shared with the asking owner, as its member is told of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Shared {
  /// The member share's number, which its readings name.
  pub id: i64,
  /// The owner key it is shared by.
  pub owner_key: String,
  /// The name its entry is handed out under.
  pub name: String,
  pub kind: ShareKind,
  pub permission: SharePermission,
  /// When it was shared, in epoch milliseconds.
  pub shared_at: i64,
}

/// A page of what is shared with the asking owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharedPage {
  pub items: Vec<Shared>,
  pub more: bool,
}

/// The query of what is shared with the asking owner.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharedQuery {
  /// An `id`: the shares made before it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u64>,
}

/// The query of a file's reading through a member share.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharedContentQuery {
  /// The file from the shared entry, the entry itself where none is named.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
  /// Handed over to be saved rather than shown.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub download: Option<bool>,
  /// The first so many bytes alone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub head: Option<u64>,
}

/// The query of `GET /places`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlacesQuery {
  /// A node, whose volumes are listed instead of this machine's places.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node_name: Option<String>,
  /// How many places were already read.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u64>,
}

/// The query of `GET /nodes`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodesQuery {
  /// How many nodes were already read.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u64>,
}

/// The query of `GET /machine`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MachineQuery {
  /// A node, read instead of this machine.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node_name: Option<String>,
}

/// The query of a folder's listing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntriesQuery {
  /// The folder, the place's top where none is named.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sort: Option<EntrySort>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub order: Option<SortOrder>,
  /// The folders alone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub folders: Option<bool>,
  /// The folder read again rather than from what was kept.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub fresh: Option<bool>,
  /// How many entries of the order were already read.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u64>,
}

/// The query of a file's reading.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContentQuery {
  /// Handed over to be saved rather than shown.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub download: Option<bool>,
  /// The first so many bytes alone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub head: Option<u64>,
}

/// The query of a file's writing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteQuery {
  /// How a name already taken is settled; replaced where none is asked.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub conflict: Option<Conflict>,
}

/// The query of the share links' list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharesQuery {
  /// A `createdAt`: the links made before it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u64>,
}

/// The query of the event stream or its page.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventsQuery {
  /// The kinds wanted, a comma-separated list.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kinds: Option<String>,
  /// An event number: the events after it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u64>,
}

/// A reader's role, as the console relays it in `Owner-Role` (bmo-ui's console plugin contract, 「Roles and the
/// owner key」).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ConsoleRole {
  Administrator,
  Member,
}
