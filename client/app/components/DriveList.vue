<template>
  <!-- The open folder (docs/files.md): the shared table in the details view, and in every other
       view the entries laid out as that view lays them, with `..` at the head of the list below a
       place's top. Entries are chosen, opened, dragged onto a folder, renamed in place and offered
       their actions on a right press or a held finger; files dropped from the desktop land in the
       folder they are dropped on, or in the open one. A line at the foot says how many entries the
       folder holds and how many are chosen, and a page more is asked for as the list nears its end. -->
  <UContextMenu :items="props.menu" :disabled="choosing" @update:open="onMenuOpen">
    <div
      ref="root"
      class="relative flex min-h-0 flex-col"
      @pointerdown.capture="onPointerKind"
      @click.capture="onClickModifiers"
      @contextmenu.capture="notePress"
      @dragenter="onDragEnter"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <div v-if="props.failure !== ''" class="flex flex-1 flex-col items-center justify-center gap-3 p-4 text-center" role="alert">
        <p class="text-sm text-error">{{ props.failure }}</p>
        <UButton color="neutral" variant="outline" size="sm" icon="i-material-symbols-refresh" :label="$t('drive.action.refresh')" @click="emit('retry')" />
      </div>

      <div
        v-else-if="props.view === 'details'"
        ref="scroller"
        class="relative flex min-h-0 flex-1 flex-col select-none"
        :aria-label="$t('drive.aria.list')"
        @scroll.capture="onScroll"
        @dblclick="onRowOpen"
        @keydown.enter="onRowOpen"
        @dragstart="onDragStart"
        @pointerdown="onMarqueeStart"
      >
        <DataTable :data="rows" :columns="columns" layout="dense" class="flex-1" selectable :row-class="rowClass" @select="(row) => onRowPress(row)">
          <template v-for="column in SORTED" :key="column" #[`${column}-header`]>
            <button
              type="button"
              class="inline-flex items-center gap-1 rounded-sm hover:text-primary active:text-primary/80"
              :class="column === 'size' ? 'w-full justify-end' : ''"
              :aria-label="$t('drive.aria.sort_by', { name: headerLabel(column) })"
              @click.stop="emit('sort', column)"
            >
              <span>{{ headerLabel(column) }}</span>
              <UIcon :name="sortIcon(column)" class="size-4 shrink-0" :class="props.sort.by === column ? 'text-highlighted' : 'text-muted'" />
            </button>
          </template>

          <!-- A folder with nothing in it says so below, so the table never draws a word of its own. -->
          <template #empty><span /></template>

          <template #name-cell="{ row }">
            <SkeletonStatus v-if="isPending(row.original)" class="flex h-7 items-center gap-2" :label="$t('drive.status.list_loading')">
              <SkeletonBar class="size-5 shrink-0 rounded-sm" />
              <SkeletonBar class="h-4 w-40 rounded-sm" />
            </SkeletonStatus>
            <span
              v-else
              class="flex h-7 min-w-0 items-center gap-2"
              :class="isCutRow(row.original) ? 'opacity-50' : ''"
              :data-entry="isParent(row.original) ? undefined : row.original.name"
              :data-parent="isParent(row.original) ? '' : undefined"
              :data-folder="row.original.kind === 'folder' ? '' : undefined"
              :draggable="!isParent(row.original) && props.renaming !== row.original.name"
            >
              <!-- The box stands on the line of the toolbar's choice of all, which its cell's own room puts a step further out. -->
              <UCheckbox v-if="choosing && !isParent(row.original)" :model-value="selected.has(row.original.name)" tabindex="-1" class="pointer-events-none -ms-1 shrink-0" :aria-label="row.original.name" />
              <UIcon :name="isParent(row.original) ? PARENT_ICON : entryIcon(row.original)" class="size-5 shrink-0" />
              <FilesNameField
                v-if="props.renaming === row.original.name"
                :name="row.original.name"
                :folder="row.original.kind === 'folder'"
                @rename="emit('rename', row.original, $event)"
                @cancel="emit('rename-cancel')"
              />
              <span v-else class="min-w-0 truncate" :class="row.original.kind === 'other' ? 'text-muted' : 'text-highlighted'">{{ row.original.name }}</span>
              <span v-if="row.original.link !== null && props.renaming !== row.original.name" class="flex min-w-0 shrink items-center gap-1 text-muted" :title="$t('drive.label.link_target', { target: row.original.link })">
                <UIcon :name="LINK_ICON" class="size-4 shrink-0" :aria-label="$t('drive.label.link_target', { target: row.original.link })" />
                <span class="min-w-0 truncate text-sm">{{ row.original.link }}</span>
              </span>
            </span>
          </template>

          <template #modified-cell="{ row }">
            <span class="flex h-7 items-center tabular-nums text-muted">
              <SkeletonBar v-if="isPending(row.original)" class="h-4 w-32 rounded-sm" />
              <template v-else-if="row.original.modified !== null">{{ formatDateClock(row.original.modified / 1000) }}</template>
            </span>
          </template>

          <template #size-cell="{ row }">
            <span class="flex h-7 items-center justify-end tabular-nums text-muted">
              <SkeletonBar v-if="isPending(row.original)" class="h-4 w-16 rounded-sm" />
              <template v-else-if="row.original.size !== null">{{ listSize(row.original.size) }}</template>
            </span>
          </template>
        </DataTable>

        <div v-if="marquee !== null" class="pointer-events-none absolute border border-primary bg-primary/10" :style="marqueeStyle" />
      </div>

      <div
        v-else
        ref="scroller"
        class="relative min-h-0 flex-1 overflow-y-auto overscroll-contain"
        :class="props.view === 'content' ? '' : 'p-2'"
        :aria-label="$t('drive.aria.list')"
        role="listbox"
        aria-multiselectable="true"
        @scroll="onScroll"
        @click="onItemPress"
        @dblclick="onItemOpen"
        @keydown.enter="onItemOpen"
        @dragstart="onDragStart"
        @pointerdown="onMarqueeStart"
      >
        <div :class="GRIDS[props.view]">
          <template v-for="(entry, at) in rows" :key="isPending(entry) ? `pending-${at}` : isParent(entry) ? '..' : entry.name">
            <SkeletonStatus v-if="isPending(entry)" as="div" :class="SKELETONS[props.view].box" :label="$t('drive.status.list_loading')">
              <SkeletonBar class="shrink-0 rounded-md" :style="{ width: `${skeletonMark}px`, height: `${skeletonMark}px` }" />
              <SkeletonBar class="h-4 w-20 rounded-sm" />
            </SkeletonStatus>
            <DriveItem
              v-else
              :entry="entry"
              :at="props.at"
              :view="props.view"
              :parent="isParent(entry)"
              :selected="!isParent(entry) && selected.has(entry.name)"
              :cut="isCutRow(entry)"
              :dropping="dropOn === entry.name"
              :choosing="choosing"
              :renaming="props.renaming === entry.name"
              :class="props.view === 'list' ? 'break-inside-avoid' : ''"
              @rename="emit('rename', entry, $event)"
              @rename-cancel="emit('rename-cancel')"
            />
          </template>
        </div>

        <div v-if="marquee !== null" class="pointer-events-none absolute border border-primary bg-primary/10" :style="marqueeStyle" />
      </div>

      <p
        v-if="props.failure === '' && props.entries !== null && props.entries.length === 0"
        class="pointer-events-none absolute inset-x-0 top-1/2 flex justify-center p-4 text-sm text-muted"
        role="status"
      >
        {{ $t("drive.status.empty_folder") }}
      </p>

      <!-- Files carried in from the desktop: the whole list says where they will land. -->
      <div
        v-if="dropFiles && dropOn === null"
        class="pointer-events-none absolute inset-2 flex items-center justify-center rounded-lg border-2 border-dashed border-primary bg-primary/10 text-sm font-medium text-primary"
        role="status"
      >
        {{ $t("drive.status.drop_here") }}
      </div>

      <slot name="foot" />

      <p class="flex h-8 shrink-0 items-center gap-2 border-t border-default px-4 text-sm tabular-nums text-muted" role="status">
        <SkeletonBar v-if="props.entries === null && props.loading" class="h-4 w-24 rounded-sm" />
        <template v-else-if="props.entries !== null">{{ footer }}</template>
      </p>
    </div>
  </UContextMenu>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useDragSelect } from "bmo-ui/app/domain/dragselect";
import { ENTRIES_DRAG, VIEW_MARK, carriesFiles, entryIcon, isCut, listSize } from "../domain/files";
import { useHoldMenu } from "bmo-ui/app/domain/holdmenu";
import { countText } from "bmo-ui/app/domain/numbers";
import { formatDateClock } from "bmo-ui/app/domain/time";

import type { FileEntry, FileSortKey } from "../../shared/types";
import type { FilesClip, FilesSort, FilesView, FolderAt } from "../domain/files";
import type { ContextMenuItem, TableColumn } from "@nuxt/ui";

interface Props {
  // The folder the entries are of.
  at: FolderAt | null;
  // The entries read so far; null before the first page.
  entries: FileEntry[] | null;
  total: number;
  // Whether the first page is being read, and whether a further one is.
  loading: boolean;
  loadingMore: boolean;
  failure: string;
  view: FilesView;
  sort: FilesSort;
  // The entry the preview shows, which the foot names where it is the only one chosen.
  chosen: string | null;
  clip: FilesClip | null;
  // The entry whose name is a field, being renamed.
  renaming: string | null;
  // What the context menu offers, for the entry it was asked over — or the folder, over nothing.
  menu: ContextMenuItem[][];
}

/** How a press on an entry was made: toggling it into the choice, extending the choice to it, or a finger. */
export interface FilesPress {
  toggle: boolean;
  range: boolean;
  touch: boolean;
}

const { t } = useI18n();
const props = defineProps<Props>();
const selected = defineModel<Set<string>>("selected", { required: true });
const choosing = defineModel<boolean>("choosing", { required: true });
const emit = defineEmits<{
  press: [entry: FileEntry, how: FilesPress];
  open: [entry: FileEntry];
  parent: [];
  sort: [by: FileSortKey];
  more: [];
  retry: [];
  rename: [entry: FileEntry, name: string];
  "rename-cancel": [];
  // The entry a context menu is asked over, or null over the folder itself.
  context: [entry: FileEntry | null];
  // Entries of the list begin to be dragged, from this one.
  drag: [entry: FileEntry, event: DragEvent];
  // Something dropped: on a folder of the list, or on the open folder where `into` is null.
  drop: [into: FileEntry | null, transfer: DataTransfer, copy: boolean];
}>();

const SORTED: readonly FileSortKey[] = ["name", "modified", "size"];
const PARENT_ICON = "i-material-symbols-drive-folder-upload-outline";

// How each view lays its entries out. A list flows down its columns as a file explorer's does.
const GRIDS: Record<Exclude<FilesView, "details">, string> = {
  huge: "grid grid-cols-[repeat(auto-fill,minmax(14rem,1fr))] gap-1",
  large: "grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-1",
  medium: "grid grid-cols-[repeat(auto-fill,minmax(6.5rem,1fr))] gap-1",
  small: "grid grid-cols-[repeat(auto-fill,minmax(14rem,1fr))] gap-x-2",
  list: "columns-[14rem] gap-x-2",
  tiles: "grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-1",
  content: "flex flex-col"
};

const SKELETONS: Record<Exclude<FilesView, "details">, { box: string }> = {
  huge: { box: "flex flex-col items-center gap-2 p-2" },
  large: { box: "flex flex-col items-center gap-2 p-2" },
  medium: { box: "flex flex-col items-center gap-1 p-2" },
  small: { box: "flex items-center gap-2 px-2 py-1" },
  list: { box: "flex items-center gap-2 px-2 py-1" },
  tiles: { box: "flex items-center gap-3 p-2" },
  content: { box: "flex items-center gap-3 border-b border-default px-4 py-2" }
};

const skeletonMark = computed(() => VIEW_MARK[props.view]);

// A row standing for an entry still being read.
interface Pending {
  pending: true;
}

// The mark beside an entry that is a link, with the target it names.
const LINK_ICON = "i-material-symbols-link";

// The row heading the list that goes to the folder above.
const PARENT_ROW: FileEntry = { name: "..", kind: "folder", size: null, modified: null, link: null };

function isPending(row: FileEntry | Pending): row is Pending {
  return "pending" in row;
}

// The table hands its rows back as reactive copies, so the row is known by what it was made from.
function isParent(row: FileEntry): boolean {
  return toRaw(row) === PARENT_ROW;
}

const root = useTemplateRef<HTMLElement>("root");
const scroller = useTemplateRef<HTMLElement>("scroller");

// The pointer the last press came from: on a touch screen one tap opens.
const pointer = ref("mouse");

function onPointerKind(event: PointerEvent): void {
  pointer.value = event.pointerType;
  pressedAt = event.target;
}

const rows = computed<FileEntry[]>(() => {
  const pending = (count: number): FileEntry[] => Array.from({ length: count }, () => ({ pending: true }) as unknown as FileEntry);
  const parent = props.at !== null && props.at.path !== "" ? [PARENT_ROW] : [];

  // The server draws the frame and no skeleton: the rows stand for a read only once it has begun.
  if (props.entries === null) {
    return props.loading ? pending(6) : [];
  }

  return [...parent, ...props.entries, ...(props.loadingMore ? pending(3) : [])];
});

const columns = computed<TableColumn<FileEntry>[]>(() => [
  { id: "name", header: t("drive.label.name"), meta: { class: { th: "w-full ps-4", td: "w-full max-w-0 ps-4" } } },
  { id: "modified", header: t("drive.label.modified") },
  { id: "size", header: t("drive.label.size"), meta: { class: { th: "text-end pe-4", td: "text-end pe-4" } } }
]);

// A column's name, as the table's own column says it.
function headerLabel(column: FileSortKey): string {
  const header = columns.value.find((one) => one.id === column)?.header;

  return typeof header === "string" ? header : "";
}

function sortIcon(column: FileSortKey): string {
  if (props.sort.by !== column) return "i-material-symbols-unfold-more";

  return props.sort.order === "asc" ? "i-material-symbols-arrow-upward" : "i-material-symbols-arrow-downward";
}

function isCutRow(row: FileEntry): boolean {
  return !isPending(row) && !isParent(row) && props.at !== null && isCut(props.clip, props.at, row.name);
}

function rowClass(row: FileEntry): string {
  if (isPending(row) || isParent(row)) return "";
  if (dropOn.value === row.name) return "bg-primary/15";

  return selected.value.has(row.name) ? "bg-accented/60" : "";
}

// ---- Pressing and opening -----------------------------------------------------------------------

// The keys held at the last press, which the table's own press does not hand on.
let modifiers = { toggle: false, range: false };

function onClickModifiers(event: MouseEvent): void {
  // The tap a finger sends as it lifts from a hold that opened the menu is the hold's, not a press.
  if (Date.now() - menuOpenedAt < AFTER_MENU_MS) {
    event.preventDefault();
    event.stopPropagation();
    menuOpenedAt = 0;

    return;
  }

  modifiers = { toggle: event.ctrlKey || event.metaKey, range: event.shiftKey };
}

function press(entry: FileEntry): void {
  if (isParent(entry)) {
    // `..` goes up at once on a finger; a pointer opens it as it opens a folder, with a double press.
    if (pointer.value === "touch") emit("parent");

    return;
  }

  emit("press", entry, { ...modifiers, touch: pointer.value === "touch" });
}

function onRowPress(row: FileEntry): void {
  if (!isPending(row)) press(row);
}

// The entry an event happened on, in whichever view.
function entryAt(target: EventTarget | null): FileEntry | null {
  if (!(target instanceof Element)) return null;

  const tableRow = target.closest("tbody tr");

  if (tableRow !== null) {
    const at = Array.prototype.indexOf.call(tableRow.parentElement?.children ?? [], tableRow);
    const row = rows.value[at];

    return row === undefined || isPending(row) ? null : row;
  }

  const item = target.closest<HTMLElement>("[data-entry],[data-parent]");

  if (item === null) return null;
  if (item.hasAttribute("data-parent")) return PARENT_ROW;

  return props.entries?.find((one) => one.name === item.dataset.entry) ?? null;
}

function onItemPress(event: MouseEvent): void {
  const entry = entryAt(event.target);

  if (entry !== null) press(entry);
}

function open(entry: FileEntry): void {
  if (isParent(entry)) emit("parent");
  else emit("open", entry);
}

// A double press, or Enter, opens what the entry is.
function onRowOpen(event: Event): void {
  const entry = entryAt(event.target);

  if (entry !== null) open(entry);
}

function onItemOpen(event: Event): void {
  const entry = entryAt(event.target);

  if (entry !== null) {
    event.preventDefault();
    open(entry);
  }
}

// Where the last press, right press or hold landed: the menu says nothing of where it was asked as it
// opens, so it names what it was asked over from there.
let pressedAt: EventTarget | null = null;

function notePress(event: Event): void {
  pressedAt = event.target;
}

// A held finger opens the menu through the console's one long press; while the list is choosing, a
// hold is the choice's instead.
useHoldMenu({ surface: () => root.value, open: () => !choosing.value });

// When a held finger last opened the menu, and how long after it the finger's lift may still arrive.
let menuOpenedAt = 0;
const AFTER_MENU_MS = 1500;

function onMenuOpen(open: boolean): void {
  if (!open) return;

  if (pointer.value === "touch") menuOpenedAt = Date.now();

  const entry = entryAt(pressedAt);

  emit("context", entry === null || isParent(entry) ? null : entry);
}

// ---- Choosing a run ------------------------------------------------------------------------------

// A finger held on an entry while the list is choosing drags the choice along the entries it passes;
// that gesture is the console's one mechanism for it.
useDragSelect({
  surface: () => root.value,
  scroller: () => scroller.value,
  rowMark: "data-entry",
  keys: () => props.entries?.map((one) => one.name) ?? [],
  selected,
  press: () => (choosing.value ? "hold" : "none"),
  hold: (key) => {
    selected.value = new Set([...selected.value, key]);
  }
});

// A pointer dragged from the folder's empty space draws a box, and chooses every entry it touches.
interface Marquee {
  x: number;
  y: number;
  toX: number;
  toY: number;
  before: Set<string>;
}

const marquee = ref<Marquee | null>(null);

const marqueeStyle = computed(() => {
  const box = marquee.value;

  if (box === null) return {};

  return { left: `${Math.min(box.x, box.toX)}px`, top: `${Math.min(box.y, box.toY)}px`, width: `${Math.abs(box.toX - box.x)}px`, height: `${Math.abs(box.toY - box.y)}px` };
});

function local(event: PointerEvent): { x: number; y: number } {
  const box = scroller.value!.getBoundingClientRect();

  return { x: event.clientX - box.left + scroller.value!.scrollLeft, y: event.clientY - box.top + scroller.value!.scrollTop };
}

function onMarqueeStart(event: PointerEvent): void {
  const target = event.target as Element | null;

  if (event.pointerType !== "mouse" || event.button !== 0 || scroller.value === null || target === null) return;
  if (target.closest("[data-entry],[data-parent],tr,input,button") !== null) return;

  const at = local(event);
  const additive = event.ctrlKey || event.metaKey;

  marquee.value = { ...at, toX: at.x, toY: at.y, before: additive ? new Set(selected.value) : new Set() };
  scroller.value.setPointerCapture(event.pointerId);
  scroller.value.addEventListener("pointermove", onMarqueeMove);
  scroller.value.addEventListener("pointerup", onMarqueeEnd, { once: true });
  scroller.value.addEventListener("pointercancel", onMarqueeEnd, { once: true });

  if (!additive) selected.value = new Set();
}

function onMarqueeMove(event: PointerEvent): void {
  const box = marquee.value;
  const element = scroller.value;

  if (box === null || element === null) return;

  const at = local(event);

  box.toX = at.x;
  box.toY = at.y;

  // The list follows a box dragged to its top or bottom edge.
  const edge = element.getBoundingClientRect();

  if (event.clientY > edge.bottom - 32) element.scrollTop += 16;
  else if (event.clientY < edge.top + 32) element.scrollTop -= 16;

  const left = Math.min(box.x, box.toX) - element.scrollLeft + edge.left;
  const top = Math.min(box.y, box.toY) - element.scrollTop + edge.top;
  const right = left + Math.abs(box.toX - box.x);
  const bottom = top + Math.abs(box.toY - box.y);
  const touched = new Set(box.before);

  for (const item of element.querySelectorAll<HTMLElement>("[data-entry]")) {
    const rect = (item.closest("tr") ?? item).getBoundingClientRect();

    if (rect.right >= left && rect.left <= right && rect.bottom >= top && rect.top <= bottom) {
      touched.add(item.dataset.entry ?? "");
    }
  }

  selected.value = touched;
}

function onMarqueeEnd(): void {
  scroller.value?.removeEventListener("pointermove", onMarqueeMove);
  marquee.value = null;
}

// ---- Dragging entries, and dropping ---------------------------------------------------------------

// The folder of the list something is dragged over, by its name; null over the open folder itself.
const dropOn = ref<string | null>(null);
// Whether what is dragged over the list is files from the desktop.
const dropFiles = ref(false);
let dragDepth = 0;

function onDragStart(event: DragEvent): void {
  const entry = entryAt(event.target);

  if (entry === null || isParent(entry) || event.dataTransfer === null) {
    event.preventDefault();

    return;
  }

  emit("drag", entry, event);
}

function carriesEntries(transfer: DataTransfer | null): boolean {
  return transfer !== null && [...transfer.types].includes(ENTRIES_DRAG);
}

function folderUnder(event: DragEvent): string | null {
  const entry = entryAt(event.target);

  return entry !== null && !isParent(entry) && entry.kind === "folder" ? entry.name : null;
}

function onDragEnter(event: DragEvent): void {
  if (!carriesFiles(event.dataTransfer) && !carriesEntries(event.dataTransfer)) return;

  dragDepth += 1;
  dropFiles.value = carriesFiles(event.dataTransfer);
}

function onDragOver(event: DragEvent): void {
  const files = carriesFiles(event.dataTransfer);

  if (!files && !carriesEntries(event.dataTransfer)) return;

  event.preventDefault();

  const folder = folderUnder(event);

  // Entries of the list land only on a folder of it; files from the desktop anywhere.
  dropOn.value = folder !== null && !selected.value.has(folder) ? folder : null;

  if (event.dataTransfer !== null) {
    event.dataTransfer.dropEffect = files ? "copy" : dropOn.value === null ? "none" : event.ctrlKey || event.metaKey ? "copy" : "move";
  }
}

function onDragLeave(): void {
  dragDepth = Math.max(0, dragDepth - 1);

  if (dragDepth === 0) {
    dropOn.value = null;
    dropFiles.value = false;
  }
}

function onDrop(event: DragEvent): void {
  const into = dropOn.value;
  const files = dropFiles.value;

  dragDepth = 0;
  dropOn.value = null;
  dropFiles.value = false;

  if (event.dataTransfer === null || (!files && into === null)) return;

  event.preventDefault();
  emit("drop", into === null ? null : (props.entries?.find((one) => one.name === into) ?? null), event.dataTransfer, event.ctrlKey || event.metaKey);
}

// ---- Paging and the foot ---------------------------------------------------------------------------

// Another page is asked for while the scroll is within a screenful of the end.
function onScroll(event: Event): void {
  const box = event.target as HTMLElement;

  if (props.entries !== null && props.entries.length < props.total && !props.loadingMore && box.scrollTop + box.clientHeight * 2 >= box.scrollHeight) {
    emit("more");
  }
}

const footer = computed(() => {
  const count = countText(props.total);

  if (selected.value.size > 1) {
    return t("drive.label.chosen_many", { n: count, k: countText(selected.value.size) });
  }

  if (props.chosen !== null) {
    return t("drive.label.chosen", { n: count, name: props.chosen });
  }

  return props.entries !== null && props.entries.length < props.total ? t("drive.label.count_loaded", { n: count, loaded: countText(props.entries.length) }) : t("drive.label.count", { n: count });
});
</script>
