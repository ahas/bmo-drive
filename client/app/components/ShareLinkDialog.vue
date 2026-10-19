<template>
  <!-- One share link: the one that stands, or the making of one. It is a small form rather than a
       part of the screen lifted out, so it is in the middle at every width, and the shared dialog is
       what puts it there and answers the back gesture. -->
  <SettingsDialog v-model:open="open" :title="$t('common.title.share_link')" :description="subject">
    <ContentSkeleton v-if="loading" listClass="flex flex-col gap-3" :label="$t('common.status.share_loading')">
      <div class="flex flex-col gap-1">
        <SkeletonBar class="h-4 w-12" />
        <SkeletonBar class="h-8 w-full rounded-md" />
      </div>
    </ContentSkeleton>

    <template v-else-if="share !== null">
      <UFormField :label="$t('common.label.link')">
        <div class="flex items-center gap-2">
          <UInput :model-value="address" readonly class="min-w-0 flex-1" :ui="{ base: 'font-mono' }" :aria-label="$t('common.aria.share_address')" @focus="selectAll" />

          <UTooltip :text="$t('common.action.copy')">
            <UButton color="neutral" variant="ghost" square icon="i-material-symbols-content-copy-outline" :aria-label="$t('common.action.copy')" @click="copy" />
          </UTooltip>
        </div>
      </UFormField>

      <div class="flex flex-wrap items-center gap-2">
        <UBadge :color="expired ? 'error' : 'success'" variant="subtle" class="tabular-nums" :label="left" />
        <span class="text-sm text-muted">{{ share.expiresAt === null ? $t("common.label.no_expiry") : $t("common.label.expires_at", { time: formatStamp(share.expiresAt) }) }}</span>
      </div>
    </template>

    <UFormField v-else :label="$t('common.label.expiry')">
      <USelect v-model="span" :items="SPAN_ITEMS" value-key="value" class="w-full" :content="{ align: 'start' }" :aria-label="$t('common.label.expiry')" />
    </UFormField>

    <UAlert v-if="failed !== ''" color="error" variant="subtle" :description="failed" />

    <template v-if="!loading" #footer>
      <UButton v-if="share === null" :label="$t('common.action.create_link')" :loading="busy" @click="make" />
      <UButton v-else :label="$t('common.action.unshare')" color="error" :loading="busy" @click="revoke" />
    </template>
  </SettingsDialog>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { SHARE_SPANS } from "../../shared/config";
import { copyText } from "../domain/copy";
import { refusalText } from "../domain/refusals";
import { useIsland } from "bmo-ui/app/domain/island";
import { formatStamp, shareLeftText, useNowSeconds } from "bmo-ui/app/domain/time";
import { appUrl } from "../domain/urls";

import type { ShareSpanKey } from "../../shared/config";
import type { ShareLink, ShareMark, ShareRoad } from "../domain/share-links";

// One link at a time. Where the thing shared has one, this opens on it instead of on the form: its
// address to copy, how much of its life is left, and the one control that ends it. A link is given
// another life by being taken away and made again, which is why there is no editing here.
//
// Making one does not close this. The form gives its place to the link it just made, in the dialog
// already standing, so the address is read and copied where it was asked for.
//
// The screen hands over the road its links go by; this dialog knows nothing of what they open.

interface Props {
  road: ShareRoad;
  // The span the form offers first.
  span?: ShareSpanKey;
  // What is being shared, where the screen behind the dialog does not already say it.
  subject?: string;
}

const props = withDefaults(defineProps<Props>(), { span: "24h", subject: undefined });
const open = defineModel<boolean>("open", { default: false });
// What the screen is to say about being shared. It is told from here: this is the one place a link
// is made and taken away, so this is the one place that knows the moment either happened.
const emit = defineEmits<{ shared: [mark: ShareMark | null] }>();

const { t } = useI18n();
const SPAN_ITEMS = SHARE_SPANS.map((one) => ({ label: t(one.label), value: one.key }));

const island = useIsland(t);

const share = ref<ShareLink | null>(null);
const span = ref<ShareSpanKey>(props.span);
const loading = ref(false);
const busy = ref(false);
const failed = ref("");
// The clock the chip counts down against, moved on while the dialog stands so the line does not
// go stale in front of whoever is reading it.
const { now, sync } = useNowSeconds();

// An address somebody outside can open, which is this browser's own origin and the path this
// console is served under — a link relative to the console would mean nothing once it is sent.
const address = computed(() => (share.value === null ? "" : `${origin.value}${appUrl(props.road.page(share.value.token))}`));
const origin = computed(() => (import.meta.client ? window.location.origin : ""));

const expired = computed(() => share.value !== null && share.value.expiresAt !== null && share.value.expiresAt <= now.value);
const left = computed(() => (share.value === null ? "" : shareLeftText(t, share.value.expiresAt, now.value)));

function selectAll(event: FocusEvent): void {
  (event.target as HTMLInputElement | null)?.select();
}

async function read(): Promise<void> {
  loading.value = true;
  failed.value = "";

  try {
    const found = await props.road.read();

    share.value = found;
    emit("shared", found === null ? null : { expiresAt: found.expiresAt });
  } catch (error) {
    failed.value = refusalText(t, error, t("common.error.share_read_failed"));
  } finally {
    loading.value = false;
  }
}

async function make(): Promise<void> {
  busy.value = true;
  failed.value = "";

  try {
    const made = await props.road.make(span.value);

    share.value = made;
    // The link is a second old, so the first figure drawn against it is its whole span.
    sync();
    emit("shared", { expiresAt: made.expiresAt });
  } catch (error) {
    failed.value = refusalText(t, error, t("common.error.share_create_failed"));
  } finally {
    busy.value = false;
  }
}

async function revoke(): Promise<void> {
  if (share.value === null) return;

  busy.value = true;
  failed.value = "";

  try {
    await props.road.remove(share.value);

    share.value = null;
    emit("shared", null);
  } catch (error) {
    failed.value = refusalText(t, error, t("common.error.unshare_failed"));
  } finally {
    busy.value = false;
  }
}

async function copy(): Promise<void> {
  const done = await copyText(address.value);

  island.say(done ? { title: t("common.message.link_copied"), tone: "success" } : { title: t("common.error.link_copy_failed"), tone: "error" });
}

// What stands here is read afresh each time it opens, and is left standing as it closes: a body
// emptied on the way out is a dialog that blinks while it goes. The span the form offers is the
// screen's own again each time.
watch(open, (standing) => {
  if (!standing) {
    return;
  }

  span.value = props.span;
  sync();
  void read();
});
</script>
