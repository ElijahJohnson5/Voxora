import { create } from "zustand";
import { createPodClient } from "@/lib/api/pod-client";
import { usePodStore } from "@/stores/pod";
import type { components } from "@/lib/api/pod";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type Message = components["schemas"]["Message"];

function channelKey(podId: string, channelId: string): string {
  return `${podId}:${channelId}`;
}

function getPodClient(podId: string) {
  const conn = usePodStore.getState().pods[podId];
  if (!conn?.podUrl || !conn?.pat) throw new Error("Not connected to pod");
  return createPodClient(conn.podUrl, conn.pat);
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

interface PinsState {
  byChannel: Record<string, Message[]>;
  loading: Record<string, boolean>;

  fetchPins: (podId: string, channelId: string) => Promise<void>;
  pinMessage: (
    podId: string,
    channelId: string,
    messageId: string,
  ) => Promise<void>;
  unpinMessage: (
    podId: string,
    channelId: string,
    messageId: string,
  ) => Promise<void>;
  gatewayChannelPinsUpdate: (podId: string, channelId: string) => void;
}

export const usePinStore = create<PinsState>()((set, get) => ({
  byChannel: {},
  loading: {},

  fetchPins: async (podId, channelId) => {
    const key = channelKey(podId, channelId);

    set((state) => ({
      loading: { ...state.loading, [key]: true },
    }));

    try {
      const client = getPodClient(podId);
      const { data, error } = await client.GET(
        "/api/v1/channels/{channel_id}/pins",
        { params: { path: { channel_id: channelId } } },
      );

      if (error || !data) throw new Error("Failed to fetch pins");

      set((state) => ({
        byChannel: { ...state.byChannel, [key]: data },
        loading: { ...state.loading, [key]: false },
      }));
    } catch {
      set((state) => ({
        loading: { ...state.loading, [key]: false },
      }));
    }
  },

  pinMessage: async (podId, channelId, messageId) => {
    try {
      const client = getPodClient(podId);
      const { data, error } = await client.PUT(
        "/api/v1/channels/{channel_id}/pins/{message_id}",
        {
          params: {
            path: { channel_id: channelId, message_id: messageId },
          },
        },
      );

      if (error || !data) throw new Error("Failed to pin message");

      // Optimistically add to pins list
      const key = channelKey(podId, channelId);
      set((state) => {
        const existing = state.byChannel[key] ?? [];
        if (existing.some((m) => m.id === messageId)) return state;
        return {
          byChannel: { ...state.byChannel, [key]: [...existing, data] },
        };
      });
    } catch {
      // silently fail
    }
  },

  unpinMessage: async (podId, channelId, messageId) => {
    // Optimistically remove
    const key = channelKey(podId, channelId);
    set((state) => {
      const existing = state.byChannel[key];
      if (!existing) return state;
      return {
        byChannel: {
          ...state.byChannel,
          [key]: existing.filter((m) => m.id !== messageId),
        },
      };
    });

    try {
      const client = getPodClient(podId);
      await client.DELETE(
        "/api/v1/channels/{channel_id}/pins/{message_id}",
        {
          params: {
            path: { channel_id: channelId, message_id: messageId },
          },
        },
      );
    } catch {
      // Re-fetch to restore correct state
      get().fetchPins(podId, channelId);
    }
  },

  gatewayChannelPinsUpdate: (podId, channelId) => {
    const key = channelKey(podId, channelId);
    // Invalidate cache — next time the panel opens it will re-fetch
    set((state) => {
      const next = { ...state.byChannel };
      delete next[key];
      return { byChannel: next };
    });
  },
}));
