import { create } from "zustand";
import { getGateway, usePodStore } from "@/stores/pod";
import { Opcode, type PresenceUpdatePayload } from "@/types/gateway";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type PresenceStatus = "online" | "idle" | "dnd" | "offline";

interface PresenceState {
  /** podId -> userId -> status */
  byPod: Record<string, Record<string, PresenceStatus>>;

  /** Handle PRESENCE_UPDATE from gateway */
  gatewayPresenceUpdate: (podId: string, payload: PresenceUpdatePayload) => void;
  /** Hydrate from READY payload */
  hydratePresences: (podId: string, presences: { user_id: string; status: string }[]) => void;
  /** Send own presence update via gateway op 9 */
  updateOwnPresence: (podId: string, status: PresenceStatus) => void;
  /** Clear all presence for a pod on disconnect */
  resetPod: (podId: string) => void;
  /** Clear all */
  reset: () => void;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const usePresenceStore = create<PresenceState>()((set) => ({
  byPod: {},

  gatewayPresenceUpdate: (podId, payload) => {
    set((state) => {
      const podPresences = { ...(state.byPod[podId] ?? {}) };
      const status = payload.status as PresenceStatus;

      if (status === "offline") {
        delete podPresences[payload.user_id];
      } else {
        podPresences[payload.user_id] = status;
      }

      return { byPod: { ...state.byPod, [podId]: podPresences } };
    });
  },

  hydratePresences: (podId, presences) => {
    const podPresences: Record<string, PresenceStatus> = {};
    for (const p of presences) {
      if (p.status !== "offline") {
        podPresences[p.user_id] = p.status as PresenceStatus;
      }
    }
    set((state) => ({
      byPod: { ...state.byPod, [podId]: podPresences },
    }));
  },

  updateOwnPresence: (podId, status) => {
    const gw = getGateway(podId);
    if (!gw) return;

    // Optimistically update own entry
    const myId = usePodStore.getState().pods[podId]?.user?.id;
    if (myId) {
      set((state) => {
        const podPresences = { ...(state.byPod[podId] ?? {}) };
        if (status === "offline") {
          delete podPresences[myId];
        } else {
          podPresences[myId] = status;
        }
        return { byPod: { ...state.byPod, [podId]: podPresences } };
      });
    }

    gw.send({ op: Opcode.PRESENCE_UPDATE, d: { status } });
  },

  resetPod: (podId) => {
    set((state) => {
      const next = { ...state.byPod };
      delete next[podId];
      return { byPod: next };
    });
  },

  reset: () => {
    set({ byPod: {} });
  },
}));
