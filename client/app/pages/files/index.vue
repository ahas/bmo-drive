<template>
  <!-- The file explorer in three parts: the folder tree, the open folder and the preview, under one
       toolbar. Where there is room they stand side by side behind a splitter and the two at the
       sides fold away; where there is not, the folder stands alone, the path takes a line of its own
       under the toolbar, and the other two rise from the bottom edge. -->
  <UDashboardPanel id="files" :ui="{ body: 'p-0 sm:p-0 gap-0 sm:gap-0 overflow-y-hidden [&>*]:max-w-none' }">
    <template #header>
      <UDashboardNavbar :title="$t('drive.title')" icon="i-material-symbols-folder-open-outline" />
    </template>

    <template #body>
      <ToolBar :label="$t('drive.aria.toolbar')">
        <!-- Each button stands in a cell of its own, the toolbar's small button centred in it. -->
        <template v-if="choosing">
          <ChooseAll :keys="entryNames" :selected="selected" @choose="chooseAll" />
          <div>
            <UTooltip :text="$t('common.action.close')">
              <UButton color="neutral" variant="ghost" icon="i-material-symbols-close" :aria-label="$t('common.action.close')" @click="leaveChoosing" />
            </UTooltip>
          </div>
        </template>
        <template v-else>
          <div>
            <UTooltip :text="treeTool.label">
              <UButton color="neutral" variant="ghost" :icon="treeTool.icon" :aria-label="treeTool.label" :aria-pressed="treeTool.pressed" @click="treeTool.onSelect()" />
            </UTooltip>
          </div>
          <div>
            <UTooltip :text="$t('drive.action.back')">
              <UButton color="neutral" variant="ghost" icon="i-material-symbols-arrow-back" :aria-label="$t('drive.action.back')" :disabled="visited.length < 2" @click="goBack" />
            </UTooltip>
          </div>
        </template>

        <!-- The bar runs right to left and its steps left to right, so a path longer than the bar
             stands at its end, where the open folder is, and one that fits stands at its start. On a
             phone it stands on a line of its own below instead. -->
        <nav v-if="!narrow" class="min-w-0 flex-1" :aria-label="$t('drive.aria.path_bar')">
          <div class="flex h-full min-w-0 flex-1 items-center overflow-x-auto [direction:rtl] [scrollbar-width:none]">
            <UBreadcrumb :items="pathItems" class="mr-auto shrink-0 [direction:ltr]" :ui="{ list: 'flex-nowrap', link: 'whitespace-nowrap', item: 'shrink-0' }" />
          </div>
        </nav>
        <div v-else class="min-w-0 flex-1" />

        <div>
          <UTooltip :text="$t('drive.action.new_folder')">
            <UButton color="neutral" variant="ghost" icon="i-material-symbols-create-new-folder-outline" :aria-label="$t('drive.action.new_folder')" :disabled="!canChange" @click="newFolder()" />
          </UTooltip>
        </div>
        <div>
          <UDropdownMenu :items="uploadItems" :content="{ align: 'end' }">
            <UTooltip :text="$t('drive.action.upload')">
              <UButton color="neutral" variant="ghost" icon="i-material-symbols-upload" :aria-label="$t('drive.action.upload')" :disabled="!canChange" :loading="uploading !== null" />
            </UTooltip>
          </UDropdownMenu>
        </div>
        <ActionGroup :actions="editActions" />

        <div>
          <UDropdownMenu :items="viewItems" :content="{ align: 'end' }">
            <UTooltip :text="$t('drive.action.view')" :disabled="!narrow">
              <UButton color="neutral" variant="ghost" :icon="VIEW_ICONS[view]" :label="narrow ? undefined : $t(VIEW_LABELS[view])" trailing-icon="i-material-symbols-arrow-drop-down" :aria-label="$t('drive.action.view')" />
            </UTooltip>
          </UDropdownMenu>
        </div>

        <div>
          <UTooltip :text="$t('drive.action.refresh')">
            <UButton color="neutral" variant="ghost" icon="i-material-symbols-refresh" :aria-label="$t('drive.action.refresh')" :loading="rereading" @click="reread" />
          </UTooltip>
        </div>
        <div v-if="!narrow">
          <UTooltip :text="previewTool.label">
            <UButton color="neutral" variant="ghost" :icon="previewTool.icon" :aria-label="previewTool.label" :aria-pressed="previewTool.pressed" @click="previewTool.onSelect()" />
          </UTooltip>
        </div>
      </ToolBar>

      <!-- On a phone the path wraps onto as many lines as it needs, rather than being cut. -->
      <nav v-if="narrow" class="shrink-0 border-b border-default px-4 py-2" :aria-label="$t('drive.aria.path_bar')">
        <UBreadcrumb :items="pathItems" :ui="{ list: 'flex-wrap gap-y-1', link: 'break-all', item: 'min-w-0' }" />
      </nav>

      <p v-if="placesFailure !== ''" role="alert" class="flex items-center gap-3 p-4 text-sm text-error">
        {{ placesFailure }}
        <UButton color="neutral" variant="outline" size="sm" icon="i-material-symbols-refresh" :label="$t('drive.action.refresh')" @click="loadPlaces" />
      </p>

      <div v-if="narrow" class="flex min-h-0 flex-1 flex-col">
        <DriveList v-bind="listProps" v-model:selected="selected" v-model:choosing="choosing" class="min-h-0 flex-1" v-on="listEvents">
          <template #foot><FilesProgress :progress="uploading" /></template>
        </DriveList>

        <!-- The tree, up from the bottom edge on a phone: it only chooses what the list shows. -->
        <UDrawer v-model:open="treeDrawer" :title="$t('drive.action.tree')" :ui="FULL_SHEET">
          <template #body>
            <DriveTree v-model:expanded="expanded" v-bind="treeProps" class="min-h-0 flex-1" @open="openFromTree" @context="onTreeContext" @drop="onTreeDrop" />
          </template>
        </UDrawer>

        <!-- The preview, up from the bottom edge when a file is tapped. -->
        <UDrawer v-model:open="previewDrawer" :title="$t('drive.action.preview')" :ui="FULL_SHEET">
          <template #body>
            <DrivePreview v-bind="previewProps" class="min-h-0 flex-1" @zoom="openPicture" />
          </template>
        </UDrawer>
      </div>

      <USplitter
        v-else
        id="files-splitter"
        ref="sides"
        auto-save-id="drive-files-splitter"
        :items="SIDES"
        :ui="{ handle: SPLIT_HANDLE }"
        class="min-h-0 flex-1"
        @collapse="onSideFold($event, false)"
        @expand="onSideFold($event, true)"
      >
        <template #tree>
          <DriveTree v-model:expanded="expanded" v-bind="treeProps" class="min-h-0 min-w-0 flex-1" @open="openFolder" @context="onTreeContext" @drop="onTreeDrop" />
        </template>

        <template #list>
          <DriveList v-bind="listProps" v-model:selected="selected" v-model:choosing="choosing" class="min-h-0 min-w-0 flex-1" v-on="listEvents">
            <template #foot><FilesProgress :progress="uploading" /></template>
          </DriveList>
        </template>

        <template #preview>
          <DrivePreview v-bind="previewProps" class="min-h-0 min-w-0 flex-1" @zoom="openPicture" />
        </template>
      </USplitter>

      <input ref="filesInput" type="file" multiple class="hidden" @change="onPicked" />
      <input ref="folderInput" type="file" webkitdirectory class="hidden" @change="onPicked" />

      <FilesConflictDialog v-model:open="conflictOpen" :names="conflictNames" @answer="onConflictAnswer" />

      <ShareLinkDialog v-model:open="shareOpen" :road="shareRoad" :spans="shareSpans" span="7d" :subject="shareTarget?.name" />
      <DriveShareList v-model:open="shareListOpen" :places="places" />

      <UModal v-model:open="propertiesOpen" :title="propertiesEntry?.name ?? ''">
        <template #body>
          <PropertyList :rows="propertiesRows" />
        </template>
      </UModal>

      <ImageViewer
        v-model:open="pictureOpen"
        :src="pictureSrc"
        :title="pictureName ?? ''"
        :can-previous="pictureAt > 0"
        :can-next="pictureAt >= 0 && pictureAt < pictures.length - 1"
        @previous="stepPicture(-1)"
        @next="stepPicture(1)"
      >
        <template #actions>
          <ActionGroup :actions="pictureActions" :properties="pictureProperties" :properties-title="pictureName" :narrow-fold="false" />
        </template>
      </ImageViewer>
    </template>
  </UDashboardPanel>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { refusalText } from "../../domain/refusals";
import { countText } from "bmo-ui/app/domain/numbers";
import { carryEntries, contentUrl, deleteEntries, endShare, makeFolder, makeShare, readListing, readNode, readPlaces, readShares, readSpace, renameEntry as renameAt, sendFile } from "../../domain/drive";
import {
  DEFAULT_SORT,
  DEFAULT_VIEW,
  ENTRIES_DRAG,
  FILES_VIEWS,
  PAGE,
  SORT_LABELS,
  TREE_PAGE,
  VIEW_LABELS,
  VIEW_ICONS,
  carriesFiles,
  crumbs,
  droppedItems,
  entryProperties,
  filesTree,
  firstPlace,
  folderKey,
  joinPath,
  keysTo,
  machineKeyOf,
  machineRowKey,
  nextSort,
  parentPath,
  pastesBeside,
  placeLabel,
  previewKind,
  readSort,
  readView,
  shownPath,
  withMachine
} from "../../domain/files";
import { OVERLAY_CLOSE_MS } from "bmo-ui/app/domain/closing";
import { useIsland } from "bmo-ui/app/domain/island";
import { askQuestion } from "bmo-ui/app/domain/question";
import { useNarrow } from "bmo-ui/app/domain/screen";
import { FULL_SHEET, SPLIT_HANDLE } from "bmo-ui/app/domain/side-parts";
import { SHARE_SPANS, spanSeconds } from "../../../shared/config";
import { browserStore, forgetPlace, rememberPlace, rememberedPlace, startingPlace } from "../../domain/place-memory";

import type { FileConflict, FileEntry, FileMachine, FilePlaces, FileShareView, FileSortKey } from "../../../shared/types";
import type { ShareRoad, ShareSpan } from "bmo-ui/app/domain/share-links";
import type { Dropped, FilesClip, FilesSort, FilesView, FolderAt, FolderChildren, FilesTreeItem } from "../../domain/files";
import type { FilesPress } from "../../components/DriveList.vue";
import type { FilesUploading } from "bmo-ui/app/components/FilesProgress.vue";
import type { BarAction, BarProperty } from "bmo-ui/app/domain/tabs";
import type { BreadcrumbItem, ContextMenuItem, DropdownMenuItem, SplitterItem } from "@nuxt/ui";

// The Files screen (docs/files.md). The place, the folder, the chosen
// file, the view and the sort live in the address; how wide the parts are, and the folder each viewer
// last opened, belong to this browser.

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const narrow = useNarrow();
const island = useIsland(t);

const SIDES: SplitterItem[] = [
  { slot: "tree", minSize: 12, defaultSize: 20, collapsible: true, collapsedSize: 0 },
  { slot: "list", minSize: 30, defaultSize: 50 },
  { slot: "preview", minSize: 16, defaultSize: 30, collapsible: true, collapsedSize: 0 }
];

const places = ref<FilePlaces | null>(null);
const placesFailure = ref("");
const placesLoading = ref(false);
// The machines whose drives are being read.
const readingMachines = ref(new Set<string>());
const entries = ref<FileEntry[] | null>(null);
const total = ref(0);
const listFailure = ref("");
const loading = ref(false);
const loadingMore = ref(false);
const rereading = ref(false);
// What the tree knows of each folder's own folders, by the folder's key.
const known = ref(new Map<string, FolderChildren>());
const expanded = ref<string[]>([machineRowKey("local")]);
// The folders opened on this visit, the open one last: what the back button steps back through.
const visited = ref<FolderAt[]>([]);
const treeOpen = ref(true);
const previewOpen = ref(true);
const treeDrawer = ref(false);
const previewDrawer = ref(false);
const sides = useTemplateRef<{ panelsRef: Array<{ collapse: () => void; expand: () => void; isCollapsed: boolean }> }>("sides");
const filesInput = useTemplateRef<HTMLInputElement>("filesInput");
const folderInput = useTemplateRef<HTMLInputElement>("folderInput");

// What is chosen in the open folder, by name, and the entry a range is counted from.
const selected = ref(new Set<string>());
let anchor: string | null = null;
// Whether a finger is choosing, where every entry carries its mark and a tap toggles it.
const choosing = ref(false);
const clip = ref<FilesClip | null>(null);
const renaming = ref<string | null>(null);
const uploading = ref<FilesUploading | null>(null);
// Whether a change is under way, which holds the next one back.
const changing = ref(false);

function queryText(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

const open = computed<FolderAt | null>(() => {
  const place = queryText(route.query.place);

  return place === null || place === "" ? null : { place, path: queryText(route.query.path) ?? "" };
});
const chosenName = computed(() => queryText(route.query.file));
const sort = computed<FilesSort>(() => readSort(route.query.sort, route.query.order));
const view = computed<FilesView>(() => readView(route.query.view));
const canChange = computed(() => open.value !== null && !changing.value);

const placeName = computed(() => (open.value === null ? "" : placeLabel(t, places.value, open.value.place)));
const chosenEntry = computed(() => entries.value?.find((one) => one.name === chosenName.value) ?? null);
const chosen = computed(() => (open.value === null || chosenEntry.value === null ? null : { ...open.value, entry: chosenEntry.value }));
const entryNames = computed(() => entries.value?.map((one) => one.name) ?? []);

function pathOf(at: FolderAt, name: string): string {
  return shownPath(places.value, at.place, joinPath(at.path, name));
}

// ---- What the parts are handed -----------------------------------------------------------------

const listProps = computed(() => ({
  at: open.value,
  entries: entries.value,
  total: total.value,
  loading: loading.value,
  loadingMore: loadingMore.value,
  failure: listFailure.value,
  view: view.value,
  sort: sort.value,
  chosen: chosenEntry.value?.name ?? null,
  clip: clip.value,
  renaming: renaming.value,
  menu: listMenu.value
}));

const listEvents = {
  press: pressEntry,
  open: openEntry,
  parent: goUp,
  sort: sortBy,
  more: loadMore,
  retry: () => void loadListing(),
  rename: renameEntry,
  "rename-cancel": () => {
    renaming.value = null;
  },
  context: onListContext,
  drag: onListDrag,
  drop: onListDrop
};

const treeItems = computed(() => (places.value === null ? null : filesTree(t, places.value, known.value)));
const treeProps = computed(() => ({ items: treeItems.value, loading: placesLoading.value, open: open.value, menu: treeMenu.value }));

const previewProps = computed(() => ({
  chosen: chosen.value,
  placeName: placeName.value,
  shownPath: chosen.value === null ? "" : pathOf(chosen.value, chosen.value.entry.name)
}));

// The path bar: the place, then each folder down to the open one, each a link to its folder.
const pathItems = computed<BreadcrumbItem[]>(() => {
  const at = open.value;

  if (at === null) return [];

  const steps = crumbs(placeName.value, at.path);

  return steps.map((step, index) => ({
    label: step.label,
    ...(index === steps.length - 1 ? {} : { to: { query: addressOf({ place: at.place, path: step.path }) } })
  }));
});

const treeTool = computed<BarAction>(() =>
  narrow.value
    ? { id: "tree", label: t("drive.action.tree"), icon: "i-material-symbols-account-tree-outline", pressed: treeDrawer.value, onSelect: () => { treeDrawer.value = true; } }
    : {
        id: "tree",
        label: treeOpen.value ? t("drive.action.tree_collapse") : t("drive.action.tree_expand"),
        icon: treeOpen.value ? "i-material-symbols-left-panel-close-outline" : "i-material-symbols-left-panel-open-outline",
        pressed: treeOpen.value,
        onSelect: () => toggleSide(0)
      }
);

const previewTool = computed<BarAction>(() => ({
  id: "preview",
  label: previewOpen.value ? t("drive.action.preview_collapse") : t("drive.action.preview_expand"),
  icon: previewOpen.value ? "i-material-symbols-right-panel-close-outline" : "i-material-symbols-right-panel-open-outline",
  pressed: previewOpen.value,
  onSelect: () => toggleSide(2)
}));

// ---- The address ---------------------------------------------------------------------------------

// The address of a folder, keeping the view and the sort the screen stands in.
function addressOf(at: FolderAt, file: string | null = null): Record<string, string> {
  const query: Record<string, string> = { place: at.place };

  if (at.path !== "") query.path = at.path;
  if (file !== null) query.file = file;
  if (view.value !== DEFAULT_VIEW) query.view = view.value;
  if (sort.value.by !== DEFAULT_SORT.by || sort.value.order !== DEFAULT_SORT.order) {
    query.sort = sort.value.by;
    query.order = sort.value.order;
  }

  return query;
}

// The address with some of its parts changed in place, as a view or an order chosen is the same
// folder read differently; a part set to null is taken out.
async function go(parts: Record<string, string | null>): Promise<void> {
  const query: Record<string, string> = {};

  for (const [key, value] of Object.entries({ ...route.query, ...parts })) {
    if (typeof value === "string") query[key] = value;
  }

  await router.replace({ query });
}

// Opening a folder is a step of the browser's history, as choosing a file is.
function openFolder(at: FolderAt): void {
  void router.push({ query: addressOf(at) });
}

function openFromTree(at: FolderAt): void {
  openFolder(at);
  treeDrawer.value = false;
}

function goUp(): void {
  const at = open.value;
  const up = at === null ? null : parentPath(at.path);

  if (at !== null && up !== null) openFolder({ place: at.place, path: up });
}

// Whether the folder being opened is the back button's, which takes the open one off the trail
// rather than adding.
let steppingBack = false;
// Where in the browser's history the open folder was opened: a folder opened at an earlier place is the
// browser's own back, a later one a step forward.
let openedAt = 0;

function historyPlace(): number {
  return import.meta.client ? Number((window.history.state as { position?: unknown } | null)?.position ?? 0) : 0;
}

function goBack(): void {
  const before = visited.value.at(-2);

  if (before !== undefined) {
    steppingBack = true;
    openFolder(before);
  }
}

function setView(next: FilesView): void {
  void go({ view: next === DEFAULT_VIEW ? null : next });
}

function sortBy(by: FileSortKey): void {
  const next = nextSort(sort.value, by);
  const same = next.by === DEFAULT_SORT.by && next.order === DEFAULT_SORT.order;

  void go({ sort: same ? null : next.by, order: same ? null : next.order });
}

// ---- Choosing and opening ---------------------------------------------------------------------------

function showInPreview(name: string | null): void {
  const at = open.value;

  if (at !== null && name !== chosenName.value) void router.push({ query: addressOf(at, name) });
}

function pressEntry(entry: FileEntry, how: FilesPress): void {
  const at = open.value;

  if (at === null) return;

  if (choosing.value) {
    const next = new Set(selected.value);

    if (next.has(entry.name)) next.delete(entry.name);
    else next.add(entry.name);

    selected.value = next;

    return;
  }

  if (how.touch && entry.kind === "folder") {
    openFolder({ place: at.place, path: joinPath(at.path, entry.name) });

    return;
  }

  if (how.toggle) {
    const next = new Set(selected.value);

    if (next.has(entry.name)) next.delete(entry.name);
    else next.add(entry.name);

    selected.value = next;
    anchor = entry.name;
  } else if (how.range && anchor !== null) {
    const names = entryNames.value;
    const from = names.indexOf(anchor);
    const to = names.indexOf(entry.name);

    selected.value = new Set(from < 0 || to < 0 ? [entry.name] : names.slice(Math.min(from, to), Math.max(from, to) + 1));
  } else {
    selected.value = new Set([entry.name]);
    anchor = entry.name;
  }

  showInPreview(entry.name);

  if (how.touch && narrow.value) {
    previewDrawer.value = true;
  }
}

function openEntry(entry: FileEntry): void {
  const at = open.value;

  if (at === null) return;

  if (entry.kind === "folder") {
    openFolder({ place: at.place, path: joinPath(at.path, entry.name) });
  } else if (entry.kind === "file" && previewKind(entry.name) === "image") {
    showInPreview(entry.name);
    pictureName.value = entry.name;
    pictureOpen.value = true;
  } else if (entry.kind === "file") {
    window.open(contentUrl({ ...at, name: entry.name }), "_blank", "noopener");
  }
}

function chooseAll(all: boolean): void {
  selected.value = all ? new Set(entryNames.value) : new Set();
}

function leaveChoosing(): void {
  choosing.value = false;
  selected.value = new Set();
}

// ---- The pictures of the open folder ------------------------------------------------------------------

const pictureOpen = ref(false);
const pictureName = ref<string | null>(null);
const pictures = computed(() => (entries.value ?? []).filter((one) => one.kind === "file" && previewKind(one.name) === "image"));
const pictureAt = computed(() => pictures.value.findIndex((one) => one.name === pictureName.value));
const pictureEntry = computed(() => pictures.value[pictureAt.value] ?? null);
const pictureSrc = computed(() => (open.value === null || pictureName.value === null ? "" : contentUrl({ ...open.value, name: pictureName.value })));

function openPicture(): void {
  if (chosenEntry.value === null) return;

  pictureName.value = chosenEntry.value.name;
  pictureOpen.value = true;
}

function stepPicture(step: number): void {
  const next = pictures.value[pictureAt.value + step];

  if (next === undefined) return;

  pictureName.value = next.name;
  selected.value = new Set([next.name]);
  anchor = next.name;
  showInPreview(next.name);
}

const pictureActions = computed<BarAction[]>(() => {
  const at = open.value;
  const name = pictureName.value;

  if (at === null || name === null) return [];

  return [
    { id: "download", label: t("drive.action.download"), icon: "i-material-symbols-download", onSelect: () => window.open(contentUrl({ ...at, name }, { download: true }), "_self") },
    { id: "open", label: t("drive.action.open_tab"), icon: "i-material-symbols-open-in-new", onSelect: () => window.open(contentUrl({ ...at, name }), "_blank", "noopener") }
  ];
});

const pictureProperties = computed<BarProperty[]>(() =>
  open.value === null || pictureEntry.value === null ? [] : entryProperties(t, placeName.value, pathOf(open.value, pictureEntry.value.name), pictureEntry.value)
);

// Pictures either side are fetched ahead, so going to one shows it at once.
watch(pictureAt, (at) => {
  if (!import.meta.client || open.value === null) return;

  for (const near of [pictures.value[at - 1], pictures.value[at + 1]]) {
    if (near !== undefined) new Image().src = contentUrl({ ...open.value, name: near.name });
  }
});

// ---- Properties ---------------------------------------------------------------------------------------

const propertiesOpen = ref(false);
const propertiesEntry = ref<FileEntry | null>(null);
const propertiesRows = computed<BarProperty[]>(() =>
  open.value === null || propertiesEntry.value === null ? [] : entryProperties(t, placeName.value, pathOf(open.value, propertiesEntry.value.name), propertiesEntry.value)
);

// ---- Share links ---------------------------------------------------------------------------------------

// The entry a share link is being made or read for, and the list of every live link (docs/files.md,
// 「Share links」). A link of the file viewer is offered a week first.
const shareOpen = ref(false);
const shareListOpen = ref(false);
const shareTarget = ref<{ place: string; path: string; name: string } | null>(null);

const shareSpans = computed<ShareSpan[]>(() => SHARE_SPANS.map((span) => ({ value: span.key, label: t(span.label) })));

// The road the share dialog goes by: each link's page is the address the service answers with it.
const shareRoad = computed<ShareRoad>(() => {
  const target = shareTarget.value;
  const entry = { place: target?.place ?? "", path: target?.path ?? "" };
  const pages = new Map<string, string>();
  const known = (share: FileShareView): FileShareView => {
    pages.set(share.token, share.url);

    return share;
  };

  return {
    page: (link) => pages.get(link.token) ?? "",
    read: async () => {
      const found = (await readShares()).find((one) => one.place === entry.place && one.path === entry.path);

      return found === undefined ? null : known(found);
    },
    make: async (span) => known(await makeShare(entry.place, entry.path, spanSeconds(span))),
    remove: async (link) => endShare(link.token)
  };
});

/** The one chosen entry a link can be made for: a file or a folder, never anything else. */
function shareable(): FileEntry | null {
  const [only] = targets();
  const entry = only === undefined || targets().length !== 1 ? undefined : entries.value?.find((one) => one.name === only);

  return entry !== undefined && (entry.kind === "file" || entry.kind === "folder") ? entry : null;
}

function shareEntry(entry: FileEntry | null = shareable()): void {
  const at = open.value;

  if (at === null || entry === null || entry.kind === "other") return;

  shareTarget.value = { place: at.place, path: joinPath(at.path, entry.name), name: entry.name };
  shareOpen.value = true;
}

// ---- Reading -------------------------------------------------------------------------------------------

// The machines as read again: a node already read keeps its drives until it is read itself.
function merged(read: FilePlaces): FilePlaces {
  const before = new Map((places.value?.machines ?? []).map((one) => [one.key, one]));

  return { ...read, machines: read.machines.map((one) => (one.local ? one : (before.get(one.key) ?? one))) };
}

// The owner's drive's usage and quota, read again after a change; where the reading fails the row keeps
// what it showed.
async function refreshSpace(): Promise<void> {
  const space = places.value?.space;

  if (space === null || space === undefined || space.error !== null) return;

  try {
    const read = await readSpace(space);

    if (places.value !== null) places.value = { ...places.value, space: read };
  } catch {
    // The usage stays as it was last read.
  }
}

async function loadPlaces(): Promise<void> {
  placesFailure.value = "";
  placesLoading.value = true;

  try {
    places.value = merged(await readPlaces());

    if (open.value === null) {
      const remembered = rememberedPlace(browserStore(), places.value.viewer);
      const start = startingPlace(null, remembered, places.value, firstPlace(places.value));

      if (start !== null) {
        openedFromMemory = remembered !== null && folderKey(start) === folderKey(remembered) ? start : null;
        await router.replace({ query: addressOf(start) });
      }
    }
  } catch (error) {
    placesFailure.value = refusalText(t, error, t("drive.error.places_failed"));
  } finally {
    placesLoading.value = false;
  }
}

// One machine's drives, read now: a node's when it is first unfolded or named by the address.
async function readMachine(key: string): Promise<void> {
  if (places.value === null || readingMachines.value.has(key)) return;

  readingMachines.value = new Set(readingMachines.value).add(key);

  let read: FileMachine;

  try {
    const standing = places.value.machines.find((one) => one.key === key);

    read = key === "local" || standing === undefined ? (await readPlaces()).machines[0]! : await readNode(standing);
  } catch (error) {
    const standing = places.value.machines.find((one) => one.key === key);

    read = { ...(standing ?? { key, name: key, local: false, garage: null }), disks: null, error: refusalText(t, error, t("drive.error.machine_failed")), errorCode: null };
  } finally {
    const next = new Set(readingMachines.value);

    next.delete(key);
    readingMachines.value = next;
  }

  if (places.value !== null) places.value = withMachine(places.value, read);
}

let listAsked = 0;
// The remembered folder the screen opened at, until its first listing answers: one that can no longer be
// listed is forgotten, and the screen opens where it would with nothing remembered.
let openedFromMemory: FolderAt | null = null;

function forgetUnlisted(at: FolderAt): void {
  const remembered = openedFromMemory;

  openedFromMemory = null;

  if (remembered === null || places.value === null || folderKey(remembered) !== folderKey(at)) return;

  forgetPlace(browserStore(), places.value.viewer);

  const first = firstPlace(places.value);

  if (first !== null && (first !== at.place || at.path !== "")) void router.replace({ query: addressOf({ place: first, path: "" }) });
}

// The open folder's first page. What already stands stays while it is read again, so a sort
// changed or a fresh read never takes the list down.
async function loadListing(fresh = false): Promise<void> {
  const at = open.value;
  const mine = ++listAsked;

  if (at === null) return;

  listFailure.value = "";
  loadingMore.value = false;
  loading.value = true;

  try {
    const page = await readListing(at, { sort: sort.value.by, order: sort.value.order, after: 0, limit: PAGE, fresh });

    if (mine !== listAsked) return;

    openedFromMemory = null;
    entries.value = page.entries;
    total.value = page.total;

    // A folder that lists is the one the viewer comes back to.
    if (places.value !== null) rememberPlace(browserStore(), places.value.viewer, at);

    // What was chosen and is gone after a change is no longer chosen.
    const names = new Set(page.entries.map((one) => one.name));

    if ([...selected.value].some((name) => !names.has(name))) {
      selected.value = new Set([...selected.value].filter((name) => names.has(name)));
    }
  } catch (error) {
    if (mine !== listAsked) return;

    entries.value = null;
    listFailure.value = refusalText(t, error, t("drive.error.list_failed"));
    forgetUnlisted(at);
  } finally {
    if (mine === listAsked) loading.value = false;
  }
}

async function loadMore(): Promise<void> {
  const at = open.value;
  const mine = listAsked;
  const shown = entries.value;

  if (at === null || shown === null || loadingMore.value || shown.length >= total.value) return;

  loadingMore.value = true;

  try {
    const page = await readListing(at, { sort: sort.value.by, order: sort.value.order, after: shown.length, limit: PAGE });

    if (mine === listAsked && entries.value === shown) {
      entries.value = [...shown, ...page.entries];
      total.value = page.total;
    }
  } catch (error) {
    if (mine === listAsked) listFailure.value = refusalText(t, error, t("drive.error.list_failed"));
  } finally {
    if (mine === listAsked) loadingMore.value = false;
  }
}

// A folder's own folders for the tree, read when it is first unfolded.
async function loadFolders(at: FolderAt, fresh = false): Promise<void> {
  const key = folderKey(at);

  if (!fresh && known.value.has(key)) return;

  if (!known.value.has(key)) known.value = new Map(known.value).set(key, { state: "loading" });

  let children: FolderChildren;

  try {
    const page = await readListing(at, { folders: true, limit: TREE_PAGE, fresh });

    children = { state: "read", names: page.entries.map((one) => one.name), more: page.total - page.entries.length };
  } catch (error) {
    children = { state: "failed", reason: refusalText(t, error, t("drive.error.list_failed")) };
  }

  known.value = new Map(known.value).set(key, children);
}

// The tree unfolds down to the open folder and reads what each step holds; a node the address names
// is read first.
function revealOpen(): void {
  const at = open.value;

  if (places.value === null || at === null) return;

  const machine = places.value.machines.find((one) => one.key === machineKeyOf(at.place));

  if (machine !== undefined && machine.disks === null && machine.error === null) void readMachine(machine.key);

  const keys = keysTo(places.value, at);
  const missing = keys.filter((key) => !expanded.value.includes(key));

  if (missing.length > 0) expanded.value = [...expanded.value, ...missing];
}

async function reread(): Promise<void> {
  rereading.value = true;

  try {
    const at = open.value;
    const readNodes = (places.value?.machines ?? []).filter((one) => !one.local && one.disks !== null).map((one) => one.key);

    await Promise.all([loadPlaces(), loadListing(true), ...(at === null ? [] : [loadFolders(at, true)]), ...readNodes.map((key) => readMachine(key))]);
  } finally {
    rereading.value = false;
  }
}

// The open folder and the tree's folders a change reached, read again.
async function refreshAfter(folders: readonly FolderAt[]): Promise<void> {
  const at = open.value;
  const keys = new Set(folders.map(folderKey));
  const tree = folders.filter((one) => known.value.has(folderKey(one)));

  await Promise.all([...(at !== null && keys.has(folderKey(at)) ? [loadListing(true)] : []), ...tree.map((one) => loadFolders(one, true))]);
}

function toggleSide(index: number): void {
  const panel = sides.value?.panelsRef[index];

  if (panel === undefined) return;

  if (panel.isCollapsed) {
    panel.expand();
  } else {
    panel.collapse();
  }
}

function onSideFold(index: unknown, shown: boolean): void {
  if (index === 0) {
    treeOpen.value = shown;
  } else if (index === 2) {
    previewOpen.value = shown;
  }
}

// ---- Changing --------------------------------------------------------------------------------------------

// What a refused change's body carries, where the names collided.
function collided(error: unknown): string[] | null {
  const failed = error as { statusCode?: unknown; data?: { data?: { names?: unknown }; names?: unknown } } | null;
  const names = failed?.data?.data?.names ?? failed?.data?.names;

  return failed?.statusCode === 409 && Array.isArray(names) ? (names as string[]) : null;
}

const conflictOpen = ref(false);
const conflictNames = ref<string[]>([]);
let conflictAnswer: ((choice: Exclude<FileConflict, "refuse"> | null) => void) | null = null;

/** The names a change would land on, asked about once: how to settle them, or null to give it up. */
function askConflict(names: string[]): Promise<Exclude<FileConflict, "refuse"> | null> {
  conflictNames.value = names;
  conflictOpen.value = true;

  return new Promise((done) => {
    conflictAnswer = done;
  });
}

function onConflictAnswer(choice: Exclude<FileConflict, "refuse"> | null): void {
  conflictAnswer?.(choice);
  conflictAnswer = null;
}

/** A change run once refusing collisions, and again settled as asked where names collided. */
async function settled<T>(run: (conflict: FileConflict) => Promise<T>, first: FileConflict = "refuse"): Promise<T | null> {
  try {
    return await run(first);
  } catch (error) {
    const names = collided(error);

    if (names === null || first !== "refuse") throw error;

    const choice = await askConflict(names);

    return choice === null ? null : await run(choice);
  }
}

/** A change held so only one runs at a time, its failure said where it stands. */
async function change(work: () => Promise<void>): Promise<void> {
  if (changing.value) return;

  changing.value = true;

  try {
    await work();
  } catch (error) {
    island.say({ tone: "error", title: refusalText(t, error, t("drive.error.change_failed")) });
  } finally {
    changing.value = false;
    void refreshSpace();
  }
}

/** The names an action takes: those chosen, or the one the menu was asked over where it is not among them. */
function targets(): string[] {
  return [...selected.value];
}

function newFolder(into: FolderAt | null = open.value): void {
  if (into === null) return;

  void change(async () => {
    const made = await makeFolder(into.place, joinPath(into.path, t("drive.action.new_folder")), "rename");

    await refreshAfter([into]);
    await menusGone();

    // The new folder's name is a field at once, as a file explorer makes it.
    if (made !== null && open.value !== null && folderKey(open.value) === folderKey(into)) {
      selected.value = new Set([made]);
      renaming.value = made;
    }
  });
}

function startRename(): void {
  const [only] = targets();

  if (only === undefined || targets().length !== 1) return;

  void menusGone().then(() => {
    if (selected.value.size === 1 && selected.value.has(only)) renaming.value = only;
  });
}

// A name field opened from a menu waits for the menu to be gone. An open menu keeps the keyboard
// inside itself and, as it leaves, hands it back to what opened it in a task of its own, which would
// take it from the field the moment the field had taken it.
async function menusGone(): Promise<void> {
  const until = Date.now() + OVERLAY_CLOSE_MS;

  while (document.querySelector("[role='menu']") !== null && Date.now() < until) {
    await new Promise((settle) => requestAnimationFrame(settle));
  }

  await new Promise((settle) => setTimeout(settle, 0));
}

function renameEntry(entry: FileEntry, name: string): void {
  const at = open.value;

  renaming.value = null;

  if (at === null || name === "" || name === entry.name) return;

  void change(async () => {
    await renameAt(at.place, joinPath(at.path, entry.name), name);
    await refreshAfter([at]);
    selected.value = new Set([name]);
    showInPreview(name);
  });
}

function deleteChosen(): void {
  const at = open.value;
  const names = targets();

  if (at === null || names.length === 0) return;

  void change(async () => {
    const sure = await askQuestion({
      title: names.length === 1 ? t("drive.message.delete_confirm", { name: names[0] }) : t("drive.message.delete_confirm_many", { n: countText(names.length) }),
      effect: t("drive.message.delete_warning"),
      confirm: t("drive.action.delete")
    });

    if (!sure) return;

    await deleteEntries(at.place, names.map((name) => joinPath(at.path, name)));
    await refreshAfter([at]);
    island.say({ tone: "success", title: t("drive.message.deleted", { n: countText(names.length) }) });
  });
}

function toClip(mode: FilesClip["mode"]): void {
  const at = open.value;
  const names = targets();

  if (at !== null && names.length > 0) clip.value = { mode, place: at.place, paths: names.map((name) => joinPath(at.path, name)) };
}

/** Entries of one place carried into a folder, copied or moved, with any collision asked about. */
async function carry(from: { place: string; paths: string[] }, into: FolderAt, move: boolean, first: FileConflict = "refuse"): Promise<void> {
  const done = await settled((conflict) => carryEntries(from, into, move, conflict), first);

  if (done === null) return;

  const sources = from.paths.map((path) => ({ place: from.place, path: parentPath(path) ?? "" }));

  await refreshAfter([into, ...sources]);
  island.say({ tone: "success", title: t(move ? "drive.message.moved" : "drive.message.copied", { n: countText(done.length) }) });
}

function paste(into: FolderAt | null = open.value): void {
  const held = clip.value;

  if (into === null || held === null) return;

  void change(async () => {
    // A copy pasted where it came from keeps both, as a file explorer makes it.
    const beside = held.mode === "copy" && pastesBeside(held, into);

    await carry({ place: held.place, paths: held.paths }, into, held.mode === "cut", beside ? "rename" : "refuse");

    if (held.mode === "cut") clip.value = null;
  });
}

async function copyPaths(paths: string[]): Promise<void> {
  try {
    await navigator.clipboard.writeText(paths.join("\n"));
    island.say({ tone: "success", title: t("drive.message.path_copied") });
  } catch (error) {
    island.say({ tone: "error", title: refusalText(t, error, t("drive.error.change_failed")) });
  }
}

// ---- Uploading --------------------------------------------------------------------------------------------


/** What was dropped or picked, uploaded into a folder: each folder made — merged where it stands — and each file sent. */
function upload(into: FolderAt, items: Dropped[]): void {
  const files = items.filter((one): one is Extract<Dropped, { kind: "file" }> => one.kind === "file");
  const all = files.reduce((sum, one) => sum + one.file.size, 0);

  if (items.length === 0) return;

  void change(async () => {
    let sentBefore = 0;
    let done = 0;
    // How collisions are settled once asked: for every file after the first that collides.
    let decided: FileConflict = "refuse";

    uploading.value = { done: 0, all: files.length, sent: 0, bytes: all };

    try {
      for (const item of items) {
        if (item.kind === "folder") {
          await makeFolder(into.place, joinPath(into.path, item.path), "skip");
          continue;
        }

        const sent: FileConflict | null = await settled(async (conflict) => {
          await sendFile(
            into,
            item.path,
            item.file,
            conflict,
            (bytes) => {
              uploading.value = { done, all: files.length, sent: sentBefore + bytes, bytes: all };
            },
            t("drive.error.upload_failed")
          );

          return conflict;
        }, decided);

        if (sent === null) break;

        decided = sent;
        sentBefore += item.file.size;
        done += 1;
        uploading.value = { done, all: files.length, sent: sentBefore, bytes: all };
      }

      if (done > 0) island.say({ tone: "success", title: t("drive.message.uploaded", { n: countText(done) }) });
    } finally {
      uploading.value = null;
      await refreshAfter([into]);
    }
  });
}

const uploadItems = computed<DropdownMenuItem[]>(() => [
  { label: t("drive.action.upload_files"), icon: "i-material-symbols-upload-file-outline", onSelect: () => filesInput.value?.click() },
  { label: t("drive.action.upload_folder"), icon: "i-material-symbols-drive-folder-upload-outline", onSelect: () => folderInput.value?.click() }
]);

// Files picked from the browser's own chooser: a folder's arrive with their paths inside it.
function onPicked(event: Event): void {
  const input = event.target as HTMLInputElement;
  const at = open.value;
  const picked = [...(input.files ?? [])];

  input.value = "";

  if (at === null || picked.length === 0) return;

  const folders = new Set<string>();

  for (const file of picked) {
    const path = file.webkitRelativePath || file.name;
    const parts = path.split("/").slice(0, -1);

    parts.forEach((_, depth) => folders.add(parts.slice(0, depth + 1).join("/")));
  }

  upload(at, [
    ...[...folders].sort((one, other) => one.split("/").length - other.split("/").length).map((path): Dropped => ({ kind: "folder", path })),
    ...picked.map((file): Dropped => ({ kind: "file", path: file.webkitRelativePath || file.name, file }))
  ]);
}

// ---- Dragging and dropping ---------------------------------------------------------------------------------

// Entries of the list dragged from one of them: the whole choice where it is among them.
function onListDrag(entry: FileEntry, event: DragEvent): void {
  const at = open.value;

  if (at === null || event.dataTransfer === null) return;

  if (!selected.value.has(entry.name)) selected.value = new Set([entry.name]);

  event.dataTransfer.setData(ENTRIES_DRAG, JSON.stringify({ place: at.place, paths: [...selected.value].map((name) => joinPath(at.path, name)) }));
  event.dataTransfer.effectAllowed = "copyMove";
}

/** Something dropped on a folder: files from the desktop uploaded, entries of the list moved — or copied, with Ctrl, or between places. */
async function dropInto(into: FolderAt, transfer: DataTransfer, copy: boolean): Promise<void> {
  if (carriesFiles(transfer)) {
    upload(into, await droppedItems(transfer));

    return;
  }

  const raw = transfer.getData(ENTRIES_DRAG);

  if (raw === "") return;

  const from = JSON.parse(raw) as { place: string; paths: string[] };

  void change(() => carry(from, into, !copy && from.place === into.place));
}

function onListDrop(entry: FileEntry | null, transfer: DataTransfer, copy: boolean): void {
  const at = open.value;

  if (at === null) return;

  void dropInto(entry === null ? at : { place: at.place, path: joinPath(at.path, entry.name) }, transfer, copy);
}

function onTreeDrop(into: FolderAt, transfer: DataTransfer, copy: boolean): void {
  void dropInto(into, transfer, copy);
}

// ---- Menus and the toolbar's actions ---------------------------------------------------------------------

const editActions = computed<BarAction[]>(() => {
  const some = selected.value.size > 0 && !changing.value;

  return [
    { id: "cut", label: t("drive.action.cut"), icon: "i-material-symbols-content-cut", disabled: !some, onSelect: () => toClip("cut") },
    { id: "copy", label: t("drive.action.copy"), icon: "i-material-symbols-content-copy", disabled: !some, onSelect: () => toClip("copy") },
    { id: "paste", label: t("drive.action.paste"), icon: "i-material-symbols-content-paste", disabled: clip.value === null || !canChange.value, onSelect: () => paste() },
    { id: "rename", label: t("drive.action.rename"), icon: "i-material-symbols-drive-file-rename-outline", disabled: selected.value.size !== 1 || changing.value, onSelect: startRename },
    { id: "delete", label: t("drive.action.delete"), icon: "i-material-symbols-delete-outline", disabled: !some, onSelect: deleteChosen },
    { id: "share", label: t("drive.action.share"), icon: "i-material-symbols-share-outline", disabled: shareable() === null, onSelect: () => shareEntry() },
    { id: "share-links", label: t("drive.action.share_links"), icon: "i-material-symbols-link", onSelect: () => (shareListOpen.value = true) }
  ];
});

const viewItems = computed<DropdownMenuItem[]>(() =>
  FILES_VIEWS.map((one) => ({ label: t(VIEW_LABELS[one]), icon: VIEW_ICONS[one], type: "checkbox" as const, checked: one === view.value, onSelect: () => setView(one) }))
);

// What the list's context menu was last asked over: an entry, or the open folder itself.
const listTarget = ref<FileEntry | null>(null);

function onListContext(entry: FileEntry | null): void {
  listTarget.value = entry;

  // A menu asked over an entry outside the choice is the menu of that entry alone.
  if (entry !== null && !selected.value.has(entry.name)) {
    selected.value = new Set([entry.name]);
    anchor = entry.name;
    showInPreview(entry.name);
  }
}

const SHORTCUT = { cut: ["ctrl", "x"], copy: ["ctrl", "c"], paste: ["ctrl", "v"], rename: ["f2"], delete: ["delete"], all: ["ctrl", "a"] };

const listMenu = computed<ContextMenuItem[][]>(() => {
  const at = open.value;
  const entry = listTarget.value;

  if (at === null) return [];

  const sortItems: ContextMenuItem[] = (["name", "modified", "size"] as const).map((by) => ({
    label: t(SORT_LABELS[by]),
    type: "checkbox" as const,
    checked: sort.value.by === by,
    onSelect: () => sortBy(by)
  }));

  if (entry === null) {
    return [
      [
        { label: t("drive.action.new_folder"), icon: "i-material-symbols-create-new-folder-outline", disabled: !canChange.value, onSelect: () => newFolder() },
        { label: t("drive.action.upload"), icon: "i-material-symbols-upload", disabled: !canChange.value, children: uploadItems.value },
        { label: t("drive.action.paste"), icon: "i-material-symbols-content-paste", kbds: SHORTCUT.paste, disabled: clip.value === null || !canChange.value, onSelect: () => paste() }
      ],
      [
        { label: t("drive.action.view"), icon: VIEW_ICONS[view.value], children: viewItems.value },
        { label: t("drive.action.sort"), icon: "i-material-symbols-sort", children: sortItems },
        { label: t("drive.action.refresh"), icon: "i-material-symbols-refresh", onSelect: () => void reread() }
      ],
      [
        { label: t("common.action.choose_all"), icon: "i-material-symbols-select-all", kbds: SHORTCUT.all, onSelect: () => chooseAll(true) },
        { label: t("drive.action.copy_path"), icon: "i-material-symbols-content-copy", onSelect: () => void copyPaths([shownPath(places.value, at.place, at.path)]) }
      ]
    ];
  }

  const many = selected.value.size > 1;
  const folder = entry.kind === "folder";
  const file = entry.kind === "file";
  const inside: FolderAt = { place: at.place, path: joinPath(at.path, entry.name) };

  return [
    [
      ...(many ? [] : [{ label: t("drive.action.open"), icon: "i-material-symbols-folder-open-outline", onSelect: () => openEntry(entry) }]),
      ...(file && !many
        ? [
            { label: t("drive.action.open_tab"), icon: "i-material-symbols-open-in-new", onSelect: () => window.open(contentUrl({ ...at, name: entry.name }), "_blank", "noopener") },
            { label: t("drive.action.download"), icon: "i-material-symbols-download", onSelect: () => window.open(contentUrl({ ...at, name: entry.name }, { download: true }), "_self") }
          ]
        : []),
      ...((file || folder) && !many ? [{ label: t("drive.action.share"), icon: "i-material-symbols-share-outline", onSelect: () => shareEntry(entry) }] : [])
    ],
    [
      { label: t("drive.action.cut"), icon: "i-material-symbols-content-cut", kbds: SHORTCUT.cut, onSelect: () => toClip("cut") },
      { label: t("drive.action.copy"), icon: "i-material-symbols-content-copy", kbds: SHORTCUT.copy, onSelect: () => toClip("copy") },
      ...(folder && !many ? [{ label: t("drive.action.paste"), icon: "i-material-symbols-content-paste", disabled: clip.value === null || !canChange.value, onSelect: () => paste(inside) }] : [])
    ],
    [
      ...(many ? [] : [{ label: t("drive.action.rename"), icon: "i-material-symbols-drive-file-rename-outline", kbds: SHORTCUT.rename, disabled: changing.value, onSelect: startRename }]),
      { label: t("drive.action.delete"), icon: "i-material-symbols-delete-outline", color: "error" as const, kbds: SHORTCUT.delete, disabled: changing.value, onSelect: deleteChosen }
    ],
    [
      { label: t("drive.action.copy_path"), icon: "i-material-symbols-content-copy", onSelect: () => void copyPaths(targets().map((name) => pathOf(at, name))) },
      ...(narrow.value && !choosing.value ? [{ label: t("drive.action.choose"), icon: "i-material-symbols-check-box-outline", onSelect: () => (choosing.value = true) }] : []),
      ...(many ? [] : [{ label: t("common.label.properties"), icon: "i-material-symbols-info-outline-rounded", onSelect: () => showProperties(entry) }])
    ]
  ].filter((group) => group.length > 0);
});

function showProperties(entry: FileEntry): void {
  propertiesEntry.value = entry;
  propertiesOpen.value = true;
}

// What the tree's context menu was last asked over.
const treeTarget = ref<FilesTreeItem | null>(null);

function onTreeContext(item: FilesTreeItem | null): void {
  treeTarget.value = item;
}

const treeMenu = computed<ContextMenuItem[][]>(() => {
  const item = treeTarget.value;

  if (item?.machine !== undefined) {
    const key = item.machine;

    return [[{ label: t("drive.action.refresh"), icon: "i-material-symbols-refresh", onSelect: () => void readMachine(key) }]];
  }

  const at = item?.folder;

  if (at === undefined) return [];

  return [
    [{ label: t("drive.action.open"), icon: "i-material-symbols-folder-open-outline", onSelect: () => openFromTree(at) }],
    [
      { label: t("drive.action.new_folder"), icon: "i-material-symbols-create-new-folder-outline", disabled: changing.value, onSelect: () => newFolder(at) },
      { label: t("drive.action.paste"), icon: "i-material-symbols-content-paste", disabled: clip.value === null || changing.value, onSelect: () => paste(at) }
    ],
    [{ label: t("drive.action.copy_path"), icon: "i-material-symbols-content-copy", onSelect: () => void copyPaths([shownPath(places.value, at.place, at.path)]) }]
  ];
});

// ---- The keyboard -------------------------------------------------------------------------------------------

// A file explorer's keys, wherever on the screen the keyboard is but a field or something open over it.
function onKey(event: KeyboardEvent): void {
  const target = event.target as HTMLElement | null;

  if (target?.closest("input, textarea, select, [contenteditable='true'], [role='dialog'], [role='menu']") || renaming.value !== null || pictureOpen.value) return;

  const command = event.ctrlKey || event.metaKey;
  const key = event.key.toLowerCase();

  if (command && key === "a") chooseAll(true);
  else if (command && key === "c") toClip("copy");
  else if (command && key === "x") toClip("cut");
  else if (command && key === "v") paste();
  else if (event.key === "F2") startRename();
  else if (event.key === "Delete") deleteChosen();
  else if (event.key === "Escape" && (selected.value.size > 0 || choosing.value)) leaveChoosing();
  else return;

  event.preventDefault();
}

onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));

// ---- Watching ---------------------------------------------------------------------------------------------------

// An unfolded folder reads its own folders, and an unfolded machine not yet read its drives; a folder
// key is a place and a path.
watch(expanded, (keys) => {
  for (const key of keys) {
    const cut = key.indexOf("\n");

    if (cut > 0 && !known.value.has(key)) {
      void loadFolders({ place: key.slice(0, cut), path: key.slice(cut + 1) });
    } else if (key.startsWith("machine:")) {
      const machine = places.value?.machines.find((one) => machineRowKey(one.key) === key);

      if (machine !== undefined && machine.disks === null && machine.error === null) void readMachine(machine.key);
    }
  }
});

// Another folder empties the list and the trail moves; another order reads the same folder again.
watch(
  () => open.value && folderKey(open.value),
  (key, before) => {
    const at = open.value;
    const place = historyPlace();

    if (at !== null) {
      const last = visited.value.at(-1);

      if (steppingBack) {
        visited.value = visited.value.slice(0, -1);
      } else if (place < openedAt) {
        // The browser's own back walks the trail back to the folder it lands on.
        const found = visited.value.findLastIndex((one) => folderKey(one) === key);

        visited.value = found < 0 ? [at] : visited.value.slice(0, found + 1);
      } else if (last === undefined || folderKey(last) !== key) {
        visited.value = [...visited.value, at];
      }
    }

    steppingBack = false;
    openedAt = place;

    if (key !== before) {
      entries.value = null;
      selected.value = new Set();
      anchor = null;
      renaming.value = null;
      choosing.value = false;
    }

    void loadListing();
    revealOpen();
  }
);

watch(sort, () => void loadListing());

// A link naming a file past the pages read so far reads on until the file stands, so the folder
// opens with it chosen.
watch([entries, chosenName], ([shown, name]) => {
  if (name !== null && shown !== null && shown.length < total.value && !shown.some((one) => one.name === name)) {
    void loadMore();
  }

  // The file the address names is the one chosen when the folder opens.
  if (name !== null && selected.value.size === 0 && shown?.some((one) => one.name === name) === true) {
    selected.value = new Set([name]);
    anchor = name;
  }
});

watch(places, () => revealOpen());

// A place the address named from the start is read here; one the screen chose itself is read by the
// watch above, as the address takes it.
onMounted(async () => {
  const named = open.value !== null;

  await loadPlaces();

  openedAt = historyPlace();

  if (named && open.value !== null) {
    visited.value = [open.value];
    void loadListing();
    revealOpen();
  }
});
</script>
