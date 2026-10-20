<template>
  <!-- Every live share link of the file viewer's entries: what each hands out, where it stands, how
       long it stands, and the two things done to a link from here — copying it and ending it. -->
  <SettingsDialog v-model:open="open" :title="$t('common.title.share_link')" wide>
    <ContentSkeleton v-if="shares === null && failed === ''" listClass="flex flex-col gap-1" :label="$t('common.status.share_loading')">
      <div v-for="row in 3" :key="row" class="flex items-center gap-3 px-2 py-2">
        <SkeletonBar class="size-6 shrink-0 rounded-md" />
        <div class="flex min-w-0 flex-1 flex-col gap-1">
          <SkeletonBar class="h-4 w-2/5" />
          <SkeletonBar class="h-4 w-3/5" />
        </div>
        <SkeletonBar class="h-6 w-16 shrink-0 rounded-md" />
      </div>
    </ContentSkeleton>

    <UAlert v-else-if="failed !== ''" color="error" variant="subtle" :description="failed" />

    <p v-else-if="shares !== null && shares.length === 0" class="py-4 text-center text-sm text-muted">{{ $t("drive.status.no_shares") }}</p>

    <ul v-else-if="shares !== null" class="flex flex-col gap-1" :aria-label="$t('common.title.share_link')">
      <li v-for="share in shares" :key="share.token" class="flex items-center gap-3 px-2 py-2">
        <UIcon :name="iconOf(share)" class="size-6 shrink-0" />
        <div class="flex min-w-0 flex-1 flex-col">
          <MarqueeLine>{{ share.name }}</MarqueeLine>
          <MarqueeLine class="text-sm text-muted">{{ shownPath(places, share.place, share.path) }}</MarqueeLine>
        </div>
        <UBadge :color="share.expiresAt !== null && share.expiresAt <= now ? 'error' : 'success'" variant="subtle" class="shrink-0 tabular-nums" :label="shareUntilText($t, share.expiresAt, now)" />
        <div class="flex shrink-0 items-center">
          <UTooltip :text="$t('drive.aria.copy_link_named', { name: share.name })">
            <UButton color="neutral" variant="ghost" square icon="i-material-symbols-content-copy-outline" :aria-label="$t('drive.aria.copy_link_named', { name: share.name })" @click="copy(share)" />
          </UTooltip>
          <UTooltip :text="$t('drive.aria.unshare_named', { name: share.name })">
            <UButton color="error" variant="ghost" square icon="i-material-symbols-link-off" :loading="removing === share.token" :aria-label="$t('drive.aria.unshare_named', { name: share.name })" @click="remove(share)" />
          </UTooltip>
        </div>
      </li>
    </ul>
  </SettingsDialog>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { copyText } from "../domain/copy";
import { refusalText } from "../domain/refusals";
import { endShare, readShares } from "../domain/drive";
import { entryIcon, shownPath } from "../domain/files";
import { useIsland } from "bmo-ui/app/domain/island";
import { shareUntilText } from "bmo-ui/app/domain/share-links";
import { useNowSeconds } from "bmo-ui/app/domain/time";

import type { FilePlaces, FileShareView } from "../../shared/types";

// The list is read afresh each time the dialog opens, so a link made or ended on the entry's own
// dialog is here when this is opened next. A link ended here leaves the list at once.

interface Props {
  places: FilePlaces | null;
}

defineProps<Props>();
const open = defineModel<boolean>("open", { default: false });

const { t } = useI18n();
const island = useIsland(t);
const { now, sync } = useNowSeconds();

const shares = ref<FileShareView[] | null>(null);
const failed = ref("");
const removing = ref<string | null>(null);

function iconOf(share: FileShareView): string {
  return entryIcon({ name: share.name, kind: share.kind, size: null, modified: null });
}

async function read(): Promise<void> {
  failed.value = "";

  try {
    shares.value = await readShares();
  } catch (error) {
    failed.value = refusalText(t, error, t("common.error.share_read_failed"));
  }
}

async function copy(share: FileShareView): Promise<void> {
  const done = await copyText(share.url);

  island.say(done ? { title: t("common.message.link_copied"), tone: "success" } : { title: t("common.error.link_copy_failed"), tone: "error" });
}

async function remove(share: FileShareView): Promise<void> {
  removing.value = share.token;

  try {
    await endShare(share.token);
    shares.value = (shares.value ?? []).filter((one) => one.token !== share.token);
  } catch (error) {
    island.say({ title: refusalText(t, error, t("common.error.unshare_failed")), tone: "error" });
  } finally {
    removing.value = null;
  }
}

watch(open, (standing) => {
  if (!standing) return;

  sync();
  void read();
});
</script>
