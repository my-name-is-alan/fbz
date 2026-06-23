import type { MaybeRefOrGetter } from "vue";
import { onScopeDispose, toValue, watch } from "vue";

let lockCount = 0;

function applyLock() {
  document.documentElement.classList.add("fbz-scroll-locked");
  document.body.classList.add("fbz-scroll-locked");
}

function releaseLock() {
  document.documentElement.classList.remove("fbz-scroll-locked");
  document.body.classList.remove("fbz-scroll-locked");
}

export function useBodyScrollLock(source: MaybeRefOrGetter<boolean>) {
  let lockedByScope = false;

  function setLocked(nextLocked: boolean) {
    if (nextLocked === lockedByScope) return;

    if (nextLocked) {
      lockCount += 1;
      if (lockCount === 1) applyLock();
      lockedByScope = true;
      return;
    }

    lockCount = Math.max(0, lockCount - 1);
    if (lockCount === 0) releaseLock();
    lockedByScope = false;
  }

  const stop = watch(() => toValue(source), setLocked, { immediate: true });

  onScopeDispose(() => {
    stop();
    setLocked(false);
  });
}
