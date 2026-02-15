import { create } from "zustand";
import { getGateway, usePodStore } from "@/stores/pod";
import { Opcode, type TypingStartPayload } from "@/types/gateway";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface TypingUser {
  userId: string;
  username: string;
  expiresAt: number;
}

/** Composite key for pod-scoped channel data. */
function channelKey(podId: string, channelId: string): string {
  return `${podId}:${channelId}`;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

interface TypingState {
  /** composite key (podId:channelId) → currently typing users */
  byChannel: Record<string, TypingUser[]>;

  /** Handle an incoming TYPING_START event from the gateway. */
  gatewayTypingStart: (podId: string, payload: TypingStartPayload) => void;
  /** Send a TYPING command to the gateway. */
  sendTyping: (podId: string, channelId: string) => void;
  /** Remove expired typing entries. */
  pruneExpired: () => void;
}

const TYPING_EXPIRY_MS = 8_000;

export const useTypingStore = create<TypingState>()((set, get) => ({
  byChannel: {},

  gatewayTypingStart: (podId, payload) => {
    // Ignore our own typing events
    const myId = usePodStore.getState().pods[podId]?.user?.id;
    if (payload.user_id === myId) return;

    const key = channelKey(podId, payload.channel_id);
    const expiresAt = Date.now() + TYPING_EXPIRY_MS;

    set((state) => {
      const existing = state.byChannel[key] ?? [];
      const idx = existing.findIndex((u) => u.userId === payload.user_id);

      let updated: TypingUser[];
      if (idx >= 0) {
        // Refresh expiry
        updated = existing.map((u, i) =>
          i === idx ? { ...u, expiresAt } : u,
        );
      } else {
        updated = [
          ...existing,
          { userId: payload.user_id, username: payload.username, expiresAt },
        ];
      }

      return { byChannel: { ...state.byChannel, [key]: updated } };
    });
  },

  sendTyping: (podId, channelId) => {
    const gw = getGateway(podId);
    if (!gw) return;
    gw.send({ op: Opcode.DISPATCH, t: "TYPING", d: { channel_id: channelId } });
  },

  pruneExpired: () => {
    const now = Date.now();
    set((state) => {
      let changed = false;
      const next: Record<string, TypingUser[]> = {};

      for (const [key, users] of Object.entries(state.byChannel)) {
        const filtered = users.filter((u) => u.expiresAt > now);
        if (filtered.length !== users.length) changed = true;
        if (filtered.length > 0) {
          next[key] = filtered;
        }
      }

      return changed ? { byChannel: next } : state;
    });
  },
}));
