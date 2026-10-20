<template>
  <!-- The Files screen's folder tree (docs/files.md): each machine, opening onto its
       drives and Garage, each folder opening onto its own folders as it is unfolded. A name is never
       cut: the tree is as wide as its widest row and scrolls sideways inside its panel. A folder is
       offered its actions on a right press or a held finger, and takes entries and files dropped on
       it. It reaches its panel's edges and scrolls on its own. -->
  <div class="flex min-h-0 flex-col">
    <ContentSkeleton v-if="props.items === null && props.loading" :rows="4" list-class="flex flex-col px-1 py-2" :label="$t('drive.status.tree_loading')">
      <template #default="{ index }">
        <div class="flex h-8 items-center gap-2 px-2">
          <SkeletonBar class="size-4 shrink-0 rounded-sm" />
          <SkeletonBar class="h-4 flex-1 rounded-sm" :class="index % 2 === 0 ? 'max-w-44' : 'max-w-32'" />
        </div>
      </template>
    </ContentSkeleton>

    <UContextMenu v-else-if="props.items !== null" :items="props.menu" @update:open="onMenuOpen">
      <div
        ref="surface"
        class="min-h-0 flex-1 overflow-auto overscroll-contain"
        @pointerdown.capture="notePress"
        @contextmenu.capture="notePress"
        @dragover="onDragOver"
        @dragleave="onDragLeave"
        @drop="onDrop"
      >
        <UTree
          v-model:expanded="expanded"
          :items="props.items"
          :get-key="(item) => item.value"
          color="primary"
          :model-value="chosen"
          class="w-max min-w-full px-1 py-2 select-none"
          :ui="{ link: 'w-max min-w-full', linkLabel: 'shrink-0 whitespace-nowrap' }"
          :aria-label="$t('drive.action.tree')"
          @select="onSelect"
        >
          <template #item-leading="{ item, expanded: open }">
            <SkeletonBar v-if="(item as FilesTreeItem).loading" class="size-4 shrink-0 rounded-sm" />
            <UIcon v-else :name="leadingIcon(item as FilesTreeItem, open)" class="size-4 shrink-0" :class="(item as FilesTreeItem).failed ? 'text-error' : ''" />
          </template>

          <template #item-label="{ item }">
            <SkeletonStatus v-if="(item as FilesTreeItem).loading" :label="$t('drive.status.tree_loading')">
              <SkeletonBar class="block h-4 w-24 rounded-sm" />
            </SkeletonStatus>
            <span
              v-else
              class="flex items-center gap-2 whitespace-nowrap"
              :class="dropOn === (item as FilesTreeItem).value ? 'rounded-sm outline-2 outline-primary' : ''"
              :data-row="(item as FilesTreeItem).value"
            >
              <span :class="(item as FilesTreeItem).failed && !(item as FilesTreeItem).note ? 'text-error' : ''">{{ (item as FilesTreeItem).label }}</span>
              <span v-if="(item as FilesTreeItem).note" :class="(item as FilesTreeItem).failed ? 'text-error' : 'text-muted'">{{ (item as FilesTreeItem).note }}</span>
            </span>
          </template>
        </UTree>
      </div>
    </UContextMenu>
  </div>
</template>

<script setup lang="ts">
import { folderIcon } from "bmo-ui/app/domain/file-icons";
import { ENTRIES_DRAG, carriesFiles, folderKey } from "../domain/files";
import { useHoldMenu } from "bmo-ui/app/domain/holdmenu";

import type { FilesTreeItem, FolderAt } from "../domain/files";
import type { ContextMenuItem } from "@nuxt/ui";

interface Props {
  // The tree's rows; null until the places are first read.
  items: FilesTreeItem[] | null;
  // Whether the places are being read: the server draws no skeleton for a read not yet begun.
  loading: boolean;
  // The folder open in the list, marked here.
  open: FolderAt | null;
  // What the context menu offers, for the row it was asked over.
  menu: ContextMenuItem[][];
}

const props = defineProps<Props>();
const emit = defineEmits<{
  open: [at: FolderAt];
  // The row a context menu is asked over.
  context: [item: FilesTreeItem | null];
  // Entries of the list or files from the desktop dropped on a folder of the tree.
  drop: [at: FolderAt, transfer: DataTransfer, copy: boolean];
}>();
const expanded = defineModel<string[]>("expanded", { required: true });
const surface = useTemplateRef<HTMLElement>("surface");

// Every row standing, by its key.
const byKey = computed(() => {
  const all = new Map<string, FilesTreeItem>();
  const walk = (items: readonly FilesTreeItem[]): void => {
    for (const item of items) {
      all.set(item.value, item);
      if (item.children !== undefined) walk(item.children);
    }
  };

  walk(props.items ?? []);

  return all;
});

// The row of the open folder, found among the rows standing so the tree marks the same object.
const chosen = computed(() => (props.open === null ? undefined : byKey.value.get(folderKey(props.open))));

// A folder below a place's top wears the file explorer's folder mark, open or shut as its row is.
function leadingIcon(item: FilesTreeItem, open: boolean): string {
  return item.icon === "" ? folderIcon(item.label, open) : item.icon;
}

function onSelect(event: { preventDefault: () => void }, item: unknown): void {
  const row = item as FilesTreeItem;

  if (row.folder === undefined) {
    return;
  }

  // Pressing a folder opens it in the list. The tree marks what is open from the address, not from
  // the press.
  event.preventDefault();
  emit("open", row.folder);
}

function rowUnder(target: EventTarget | null): FilesTreeItem | null {
  const key = target instanceof Element ? target.closest<HTMLElement>("[data-row]")?.dataset.row : undefined;

  return key === undefined ? null : (byKey.value.get(key) ?? null);
}

// Where the last press, right press or hold landed: the menu says nothing of where it was asked as it
// opens, so it names the row it was asked over from there.
let pressedAt: EventTarget | null = null;

function notePress(event: Event): void {
  pressedAt = event.target;
}

function onMenuOpen(open: boolean): void {
  if (open) emit("context", rowUnder(pressedAt));
}

// A held finger opens the menu through the console's one long press.
useHoldMenu({ surface: () => surface.value });

// The folder row something is dragged over.
const dropOn = ref<string | null>(null);

function onDragOver(event: DragEvent): void {
  const transfer = event.dataTransfer;
  const row = rowUnder(event.target);

  if (transfer === null || (!carriesFiles(transfer) && ![...transfer.types].includes(ENTRIES_DRAG)) || row?.folder === undefined) {
    dropOn.value = null;

    return;
  }

  event.preventDefault();
  dropOn.value = row.value;
  transfer.dropEffect = carriesFiles(transfer) || event.ctrlKey || event.metaKey ? "copy" : "move";
}

function onDragLeave(event: DragEvent): void {
  if (rowUnder(event.relatedTarget) === null) dropOn.value = null;
}

function onDrop(event: DragEvent): void {
  const row = dropOn.value === null ? undefined : byKey.value.get(dropOn.value);

  dropOn.value = null;

  if (row?.folder === undefined || event.dataTransfer === null) return;

  event.preventDefault();
  emit("drop", row.folder, event.dataTransfer, event.ctrlKey || event.metaKey);
}
</script>
