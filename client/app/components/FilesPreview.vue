<template>
  <!-- The chosen entry (docs/files.md): its name, its actions and its properties at
       the head, and below it the file as the browser can show it. -->
  <section class="flex min-h-0 flex-col" :aria-label="$t('files.action.preview')">
    <div v-if="props.chosen !== null" class="flex h-9 shrink-0 items-center gap-2 border-b border-default ps-4 pe-1 pointer-coarse:h-10">
      <UIcon :name="entryIcon(props.chosen.entry)" class="size-4 shrink-0" />
      <span class="min-w-0 flex-1 truncate text-sm font-medium text-highlighted">{{ props.chosen.entry.name }}</span>
      <ActionGroup :actions="actions" :properties="properties" :properties-title="props.chosen.entry.name" :narrow-fold="false" />
    </div>

    <p v-if="props.chosen === null" class="flex flex-1 items-center justify-center p-4 text-sm text-muted" role="status">
      {{ $t("files.status.choose_file") }}
    </p>

    <div v-else-if="props.chosen.entry.kind !== 'file'" class="flex flex-1 flex-col items-center justify-center gap-3 p-4">
      <UIcon :name="entryIcon(props.chosen.entry)" class="size-16" />
      <p class="text-sm text-muted">{{ typeLabel }}</p>
    </div>

    <!-- A picture stands fitted, and opens in the console's own picture viewer, which zooms, moves and
         goes on to the folder's other pictures. -->
    <button
      v-else-if="kind === 'image'"
      type="button"
      class="flex min-h-0 flex-1 cursor-zoom-in items-center justify-center overflow-hidden p-4 hover:bg-elevated/50 active:bg-accented/50"
      :aria-label="$t('files.action.zoom')"
      @click="emit('zoom')"
    >
      <LoadingImage :src="shownUrl" :alt="props.chosen.entry.name" eager image-class="max-h-full max-w-full object-contain" class="max-h-full" />
    </button>

    <iframe v-else-if="kind === 'pdf'" :src="shownUrl" :title="props.chosen.entry.name" class="min-h-0 w-full flex-1 border-0 bg-default" />

    <!-- A video plays in the browser's own player, which asks for it a byte range at a time, so a large
         one starts at once and seeks anywhere; one the browser cannot play falls to the download. -->
    <div v-else-if="kind === 'video' && video.state !== 'unplayable'" class="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden p-4">
      <p v-if="video.state === 'failed'" class="text-center text-sm text-error" role="alert">{{ video.reason }}</p>
      <template v-else>
        <video
          :key="shownUrl"
          :src="shownUrl"
          :aria-label="props.chosen.entry.name"
          controls
          playsinline
          preload="metadata"
          class="max-h-full max-w-full"
          @loadedmetadata="videoRead"
          @error="videoFailed"
        />
        <SkeletonStatus v-if="video.state === 'loading'" as="div" class="absolute inset-4 flex items-center" :label="$t('common.status.video_loading')">
          <SkeletonBar class="aspect-video max-h-full w-full" />
        </SkeletonStatus>
      </template>
    </div>

    <template v-else-if="kind === 'text' && text.state !== 'none'">
      <SkeletonStatus v-if="text.state === 'loading'" as="div" class="p-4" :label="$t('files.status.preview_loading')">
        <SkeletonText :lines="8" size="sm" last-width="w-1/3" />
      </SkeletonStatus>
      <p v-else-if="text.state === 'failed'" class="flex flex-1 items-center justify-center p-4 text-center text-sm text-error" role="alert">{{ text.reason }}</p>
      <template v-else-if="text.state === 'read'">
        <p v-if="text.cut" class="shrink-0 border-b border-default bg-elevated px-4 py-2 text-sm tabular-nums text-muted" role="status">
          {{ $t("files.label.text_cut", { shown: listSize(TEXT_HEAD_BYTES), whole: listSize(props.chosen.entry.size ?? 0) }) }}
        </p>
        <FilesText :text="text.text" :name="props.chosen.entry.name" />
      </template>
    </template>

    <div v-else class="flex flex-1 flex-col items-center justify-center gap-3 p-4 text-center">
      <UIcon :name="entryIcon(props.chosen.entry)" class="size-16" />
      <p class="text-sm text-muted">{{ kind === "video" ? $t("files.status.unplayable") : $t("files.status.no_preview") }}</p>
      <UButton color="neutral" variant="outline" size="sm" icon="i-material-symbols-download" :label="$t('files.action.download')" :to="downloadUrl" external />
    </div>
  </section>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { refusalText } from "../domain/refusals";
import { TEXT_HEAD_BYTES, entryIcon, entryProperties, isJson, laidOutJson, listSize, previewKind, readUrl, textOf, typeLabel as typeOf } from "../domain/files";

import type { FileEntry } from "../../shared/types";
import type { FolderAt } from "../domain/files";
import type { BarAction, BarProperty } from "bmo-ui/app/domain/tabs";

interface Props {
  // The entry chosen, with the folder it stands in; null where nothing is chosen.
  chosen: (FolderAt & { entry: FileEntry }) | null;
  // The name of the place the folder is of, and the entry's path as it is copied out.
  placeName: string;
  shownPath: string;
}

const { t } = useI18n();
const props = defineProps<Props>();
const emit = defineEmits<{ zoom: [] }>();

type TextState = { state: "loading" } | { state: "failed"; reason: string } | { state: "none" } | { state: "read"; text: string; cut: boolean };

type VideoState = { state: "loading" } | { state: "ready" } | { state: "unplayable" } | { state: "failed"; reason: string };

const text = ref<TextState>({ state: "loading" });
const video = ref<VideoState>({ state: "loading" });

const at = computed(() => (props.chosen === null ? null : { place: props.chosen.place, path: props.chosen.path, name: props.chosen.entry.name }));
const kind = computed(() => (props.chosen === null ? "none" : previewKind(props.chosen.entry.name)));
const shownUrl = computed(() => (at.value === null ? "" : readUrl(at.value)));
const downloadUrl = computed(() => (at.value === null ? "" : readUrl(at.value, { download: true })));

const actions = computed<BarAction[]>(() =>
  props.chosen?.entry.kind !== "file"
    ? []
    : [
        { id: "download", label: t("files.action.download"), icon: "i-material-symbols-download", onSelect: () => window.open(downloadUrl.value, "_self") },
        { id: "open", label: t("files.action.open_tab"), icon: "i-material-symbols-open-in-new", onSelect: () => window.open(shownUrl.value, "_blank", "noopener") }
      ]
);

const typeLabel = computed(() => (props.chosen === null ? "" : typeOf(t, props.chosen.entry)));

const properties = computed<BarProperty[]>(() => (props.chosen === null ? [] : entryProperties(t, props.placeName, props.shownPath, props.chosen.entry)));

let asked = 0;

// A text file's head, read once it is chosen; one that is not text after all stands as having no
// preview.
async function readText(): Promise<void> {
  const chosen = props.chosen;
  const mine = ++asked;

  if (!import.meta.client || chosen === null || at.value === null || chosen.entry.kind !== "file" || kind.value !== "text") {
    return;
  }

  text.value = { state: "loading" };

  try {
    const response = await fetch(readUrl(at.value, { head: TEXT_HEAD_BYTES }));

    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as { code?: unknown } | null;

      throw { data: { code: body?.code } };
    }

    const bytes = new Uint8Array(await response.arrayBuffer());
    const cut = (chosen.entry.size ?? 0) > TEXT_HEAD_BYTES;
    const read = textOf(bytes, cut);

    if (mine !== asked) return;

    text.value = read === null ? { state: "none" } : { state: "read", text: isJson(chosen.entry.name) && !cut ? laidOutJson(read) : read, cut };
  } catch (error) {
    if (mine === asked) text.value = { state: "failed", reason: refusalText(t, error, t("files.error.preview_failed")) };
  }
}

// A video whose picture the browser cannot draw — a codec it lacks, with only the sound left — is one
// it cannot play.
function videoRead(event: Event): void {
  video.value = (event.target as HTMLVideoElement).videoWidth === 0 ? { state: "unplayable" } : { state: "ready" };
}

// A file the browser could not play is told from one the server could not read by asking for its
// first byte: a refusal there is the reason, and an answer means the codec is the browser's lack.
async function videoFailed(event: Event): Promise<void> {
  const failure = (event.target as HTMLVideoElement).error;
  const mine = asked;

  if (failure === null || failure.code === MediaError.MEDIA_ERR_ABORTED || at.value === null) return;

  if (failure.code === MediaError.MEDIA_ERR_NETWORK) {
    video.value = { state: "failed", reason: t("files.error.preview_failed") };

    return;
  }

  if (failure.code === MediaError.MEDIA_ERR_DECODE) {
    video.value = { state: "unplayable" };

    return;
  }

  const response = await fetch(readUrl(at.value, { head: 1 })).catch(() => null);
  const body = response === null || response.ok ? null : ((await response.json().catch(() => null)) as { code?: unknown } | null);

  if (mine !== asked) return;

  video.value =
    response !== null && response.ok
      ? { state: "unplayable" }
      : { state: "failed", reason: refusalText(t, { data: { code: body?.code } }, t("files.error.preview_failed")) };
}

watch(
  () => at.value && `${at.value.place}\n${at.value.path}\n${at.value.name}`,
  () => {
    video.value = { state: "loading" };
    void readText();
  },
  { immediate: true }
);
</script>
