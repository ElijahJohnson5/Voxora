/**
 * Auto-idle presence manager.
 *
 * Listens for user activity (mouse, keyboard) and automatically sets
 * the user's presence to "idle" after 5 minutes of inactivity.
 * Resumes to "online" when activity is detected — unless the user
 * manually set DND.
 */

import { usePresenceStore, type PresenceStatus } from "@/stores/presence";
import { usePodStore } from "@/stores/pod";

const IDLE_TIMEOUT_MS = 5 * 60 * 1_000; // 5 minutes

let idleTimer: ReturnType<typeof setTimeout> | null = null;
let isAutoIdle = false;
let initialized = false;

/** The status the user manually chose (to avoid overriding DND). */
let manualStatus: PresenceStatus = "online";

function getConnectedPodIds(): string[] {
  const pods = usePodStore.getState().pods;
  return Object.values(pods)
    .filter((p) => p.connected)
    .map((p) => p.podId);
}

function setIdleForAllPods() {
  isAutoIdle = true;
  const podIds = getConnectedPodIds();
  for (const podId of podIds) {
    usePresenceStore.getState().updateOwnPresence(podId, "idle");
  }
}

function resumeFromIdle() {
  if (!isAutoIdle) return;
  isAutoIdle = false;
  const podIds = getConnectedPodIds();
  for (const podId of podIds) {
    usePresenceStore.getState().updateOwnPresence(podId, manualStatus);
  }
}

function resetTimer() {
  if (idleTimer) clearTimeout(idleTimer);

  // Only auto-idle if the user hasn't manually set DND
  if (manualStatus === "dnd") return;

  if (isAutoIdle) {
    resumeFromIdle();
  }

  idleTimer = setTimeout(setIdleForAllPods, IDLE_TIMEOUT_MS);
}

/**
 * Update the manually-chosen status. Called when the user explicitly
 * picks a status from the dropdown.
 */
export function setManualPresenceStatus(status: PresenceStatus) {
  manualStatus = status === "offline" ? "online" : status;
  isAutoIdle = false;

  // Reset the idle timer when the user changes status
  if (idleTimer) clearTimeout(idleTimer);
  if (manualStatus !== "dnd") {
    idleTimer = setTimeout(setIdleForAllPods, IDLE_TIMEOUT_MS);
  }
}

/**
 * Initialize the auto-idle listener. Safe to call multiple times —
 * only attaches listeners once.
 */
export function initPresenceIdle() {
  if (initialized) return;
  initialized = true;

  const events: (keyof DocumentEventMap)[] = ["mousemove", "keydown", "mousedown"];
  for (const event of events) {
    document.addEventListener(event, resetTimer, { passive: true });
  }

  // Start the initial timer
  idleTimer = setTimeout(setIdleForAllPods, IDLE_TIMEOUT_MS);
}

/**
 * Tear down listeners (useful for cleanup/testing).
 */
export function destroyPresenceIdle() {
  if (!initialized) return;
  initialized = false;

  const events: (keyof DocumentEventMap)[] = ["mousemove", "keydown", "mousedown"];
  for (const event of events) {
    document.removeEventListener(event, resetTimer);
  }

  if (idleTimer) {
    clearTimeout(idleTimer);
    idleTimer = null;
  }
}
