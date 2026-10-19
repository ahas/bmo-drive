<template>
  <!-- What a file's share link opens for somebody with no login: the entry's name, what it is, how
       long the link has left, and the one thing it offers — the download. Nothing leads anywhere
       else in this console. -->
  <ScreenFrame>
    <div class="flex min-h-0 flex-1 items-center justify-center overflow-y-auto p-4">
      <p v-if="failed !== ''" role="alert" class="text-sm text-error">{{ failed }}</p>

      <ContentSkeleton v-else-if="shared === null" listClass="flex w-full max-w-md flex-col items-center gap-4" :label="$t('common.status.share_loading')">
        <SkeletonBar class="size-16 rounded-md" />
        <SkeletonBar class="h-7 w-3/5" />
        <SkeletonBar class="h-5 w-2/5" />
        <SkeletonBar class="h-8 w-28 rounded-md" />
      </ContentSkeleton>

      <section v-else class="flex w-full max-w-md flex-col items-center gap-4 text-center">
        <UIcon :name="icon" class="size-16" />
        <h1 class="text-xl break-all">{{ shared.name }}</h1>
        <p class="text-sm text-muted tabular-nums">
          {{ shared.size === null ? $t("files.label.share_folder") : $t("files.label.size_exact", { size: listSize(shared.size), bytes: countText(shared.size) }) }}
        </p>
        <div class="flex flex-wrap items-center justify-center gap-2">
          <UBadge :color="expired ? 'error' : 'success'" variant="subtle" class="tabular-nums" :label="shareLeftText($t, shared.expiresAt, now)" />
          <span v-if="shared.expiresAt !== null" class="text-sm text-muted">{{ $t("common.label.expires_at", { time: formatStamp(shared.expiresAt) }) }}</span>
        </div>
        <UButton :to="download" external :label="$t('files.action.download')" icon="i-material-symbols-download" size="lg" />
      </section>
    </div>
  </ScreenFrame>
</template>

<script setup lang="ts">
import { countText } from "bmo-ui/app/domain/numbers";
import { formatStamp, shareLeftText, useNowSeconds } from "bmo-ui/app/domain/time";
import { useI18n } from "vue-i18n";
import { entryIcon, listSize } from "../../../domain/files";
import { refusalText } from "../../../domain/refusals";
import { appUrl } from "../../../domain/urls";
import { SHARE_ROBOTS } from "../../../../shared/share-head";

import type { SharedFileView } from "../../../../shared/types";

// The page knows its entry by the token it arrived holding and by nothing else: the place and the
// path stay on the service, and the one road it offers is under that token (docs/files.md, 「Share
// links」). It is read while the page is rendered on the server, so a chat app drawing a preview of
// the link reads the file's name without running anything.

definePageMeta({ layout: false });

const { t } = useI18n();
const route = useRoute();
const token = computed(() => String(route.params.token ?? ""));
const { now } = useNowSeconds();

const { data, error } = await useFetch<SharedFileView>(() => `/api/share/files/${encodeURIComponent(token.value)}`, { key: `file-share:${token.value}` });

const shared = computed(() => data.value ?? null);
const failed = computed(() => (error.value ? refusalText(t, error.value, t("common.error.share_not_found")) : ""));
const download = computed(() => appUrl(`/api/share/files/${encodeURIComponent(token.value)}/download`));
const expired = computed(() => shared.value !== null && shared.value.expiresAt !== null && shared.value.expiresAt <= now.value);
const icon = computed(() => (shared.value === null ? "" : entryIcon({ name: shared.value.name, kind: shared.value.kind, size: shared.value.size, modified: null })));

useHead({
  title: computed(() => shared.value?.name ?? t("screens.label.files")),
  meta: computed(() => [SHARE_ROBOTS, ...(shared.value === null ? [] : [{ property: "og:title", content: shared.value.name }])])
});
</script>
