<template>
  <!-- What a file's share link opens for somebody with no login: the entry's name, what it is, until
       when the link stands, and the one thing it offers — the download. Nothing leads anywhere else
       from here. -->
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
        <p class="text-sm text-muted">
          {{ shared.kind === "folder" ? $t("drive.label.share_folder") : $t("drive.label.plain_file") }}
        </p>
        <div class="flex flex-wrap items-center justify-center gap-2">
          <UBadge :color="expired ? 'error' : 'success'" variant="subtle" class="tabular-nums" :label="shareUntilText($t, shared.expiresAt, now)" />
          <span v-if="shared.expiresAt !== null" class="text-sm text-muted">{{ $t("drive.label.expires_at", { time: formatStamp(shared.expiresAt) }) }}</span>
        </div>
        <UButton :to="download" external :label="$t('drive.action.download')" icon="i-material-symbols-download" size="lg" />
      </section>
    </div>
  </ScreenFrame>
</template>

<script setup lang="ts">
import { shareUntilText } from "bmo-ui/app/domain/share-links";
import { formatStamp, useNowSeconds } from "bmo-ui/app/domain/time";
import { useI18n } from "vue-i18n";
import { shareContentUrl, shareRoute, sharedView } from "../../../domain/drive";
import { entryIcon } from "../../../domain/files";
import { refusalText } from "../../../domain/refusals";
import { SHARE_ROBOTS } from "../../../../shared/share-head";

import type { components } from "../../../../shared/openapi";

// The page knows its entry by the token it arrived holding and by nothing else: the place and the
// path stay on the service, and the one road it offers is under that token (docs/files.md, 「Share
// links」). It is read while the page is rendered on the server, so a chat app drawing a preview of
// the link reads the file's name without running anything.

definePageMeta({ layout: false });

const { t } = useI18n();
const route = useRoute();
const token = computed(() => String(route.params.token ?? ""));
const { now } = useNowSeconds();

const { data, error } = await useFetch<components["schemas"]["Share"]>(() => shareRoute(token.value), { key: `file-share:${token.value}` });

const shared = computed(() => (data.value ? sharedView(data.value) : null));
const failed = computed(() => (error.value ? refusalText(t, error.value, t("drive.error.share_not_found")) : ""));
const download = computed(() => shareContentUrl(token.value));
const expired = computed(() => shared.value !== null && shared.value.expiresAt !== null && shared.value.expiresAt <= now.value);
const icon = computed(() => (shared.value === null ? "" : entryIcon({ name: shared.value.name, kind: shared.value.kind, size: null, modified: null, link: null })));

useHead({
  title: computed(() => shared.value?.name ?? t("drive.title")),
  meta: computed(() => [SHARE_ROBOTS, ...(shared.value === null ? [] : [{ property: "og:title", content: shared.value.name }])])
});
</script>
