<template>
  <!-- One entry of the open folder in every view but the details view (docs/files.md): its mark at
       the view's own size — a picture as itself where the mark is large enough — and its name, with
       what the view adds beside it. The head of the list's `..` stands as one of these too. A double
       press never selects its words; the name is a field of its own while it is being renamed. -->
  <div
    role="option"
    :aria-selected="props.selected"
    :data-entry="props.parent ? undefined : props.entry.name"
    :data-parent="props.parent ? '' : undefined"
    :data-folder="props.entry.kind === 'folder' ? '' : undefined"
    :draggable="!props.parent && !props.renaming"
    tabindex="-1"
    class="group relative flex min-w-0 cursor-default rounded-md transition-colors select-none focus-inset"
    :class="[LAYOUT[props.view], props.selected ? 'bg-accented/60' : props.dropping ? 'bg-primary/15 outline-2 outline-primary' : 'hover:bg-elevated active:bg-accented/50', props.cut ? 'opacity-50' : '']"
  >
    <UCheckbox
      v-if="props.choosing && !props.parent"
      :model-value="props.selected"
      tabindex="-1"
      class="pointer-events-none shrink-0"
      :class="stacked ? 'absolute top-2 left-2' : '-ms-1'"
      :aria-label="props.entry.name"
    />

    <span class="flex shrink-0 items-center justify-center" :style="{ width: `${mark}px`, height: `${mark}px` }">
      <img
        v-if="!props.parent && props.at !== null && showsThumbnail(props.entry, mark)"
        :src="readUrl({ ...props.at, name: props.entry.name })"
        :alt="props.entry.name"
        loading="lazy"
        draggable="false"
        class="max-h-full max-w-full rounded-sm object-contain"
      />
      <UIcon v-else :name="props.parent ? PARENT_ICON : entryIcon(props.entry)" :style="{ width: `${iconSize}px`, height: `${iconSize}px` }" />
    </span>

    <span class="flex min-w-0 flex-col" :class="stacked ? 'w-full items-center text-center' : 'flex-1 justify-center'">
      <FilesNameField v-if="props.renaming" :name="props.entry.name" :folder="props.entry.kind === 'folder'" @rename="emit('rename', $event)" @cancel="emit('rename-cancel')" />
      <span
        v-else
        class="text-sm"
        :class="[stacked ? 'line-clamp-2 w-full break-all' : 'truncate', props.entry.kind === 'other' ? 'text-muted' : 'text-highlighted', props.view === 'content' ? 'font-medium' : '']"
      >
        {{ props.parent ? ".." : props.entry.name }}
      </span>

      <template v-if="!props.parent && (props.view === 'tiles' || props.view === 'content')">
        <span class="truncate text-sm text-muted">{{ typeLabel(t, props.entry) }}</span>
        <span v-if="props.view === 'tiles' && props.entry.size !== null" class="truncate text-sm text-muted tabular-nums">{{ listSize(props.entry.size) }}</span>
      </template>
    </span>

    <span v-if="props.view === 'content' && !props.parent" class="flex shrink-0 flex-col items-end justify-center text-sm text-muted tabular-nums">
      <span v-if="props.entry.modified !== null">{{ formatDateClock(props.entry.modified / 1000) }}</span>
      <span v-if="props.entry.size !== null">{{ listSize(props.entry.size) }}</span>
    </span>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { VIEW_MARK, entryIcon, listSize, readUrl, showsThumbnail, typeLabel } from "../domain/files";
import { formatDateClock } from "bmo-ui/app/domain/time";

import type { FileEntry } from "../../shared/types";
import type { FilesView, FolderAt } from "../domain/files";

interface Props {
  entry: FileEntry;
  at: FolderAt | null;
  view: Exclude<FilesView, "details">;
  // Whether this is the `..` heading the list, which goes to the folder above.
  parent?: boolean;
  selected?: boolean;
  // Whether it waits, cut, to be moved.
  cut?: boolean;
  // Whether entries dragged over it would land in it.
  dropping?: boolean;
  // Whether the list is choosing, where every entry carries its mark.
  choosing?: boolean;
  renaming?: boolean;
}

const { t } = useI18n();
const props = withDefaults(defineProps<Props>(), { parent: false, selected: false, cut: false, dropping: false, choosing: false, renaming: false });
const emit = defineEmits<{ rename: [name: string]; "rename-cancel": [] }>();

const PARENT_ICON = "i-material-symbols-drive-folder-upload-outline";

// The views whose mark stands over the name, and how each view lays an entry out.
const STACKED = new Set(["huge", "large", "medium"]);
const LAYOUT: Record<Props["view"], string> = {
  huge: "flex-col items-center gap-2 p-2",
  large: "flex-col items-center gap-2 p-2",
  medium: "flex-col items-center gap-1 p-2",
  small: "items-center gap-2 px-2 py-1",
  list: "items-center gap-2 px-2 py-1",
  tiles: "items-center gap-3 p-2",
  content: "items-center gap-3 border-b border-default px-4 py-2 rounded-none"
};

const stacked = computed(() => STACKED.has(props.view));
const mark = computed(() => VIEW_MARK[props.view]);
// A file's own mark is drawn a step inside its box, as a file explorer draws it.
const iconSize = computed(() => (mark.value >= 48 ? Math.round(mark.value * 0.8) : mark.value));
</script>
