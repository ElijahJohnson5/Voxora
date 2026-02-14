import { create } from "zustand";
import { createPodClient } from "@/lib/api/pod-client";
import { usePodStore } from "@/stores/pod";
import type { components } from "@/lib/api/pod";
import type { MessagePayload, ReactionPayload } from "@/types/gateway";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type Message = components["schemas"]["Message"];

/** A message that has been optimistically inserted but not yet confirmed. */
export interface PendingMessage {
  nonce: string;
  podId: string;
  channel_id: string;
  content: string;
  reply_to: string | null;
  created_at: string;
  author_id: string;
}

export interface Reaction {
  emoji: string;
  count: number;
  me: boolean;
}

/** Per-channel message state. */
interface ChannelMessages {
  messages: Message[];
  /** Has older messages above the current window */
  hasOlder: boolean;
  /** Has newer messages below the current window */
  hasNewer: boolean;
  loading: boolean;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Composite key for pod-scoped channel data. */
export function channelKey(podId: string, channelId: string): string {
  return `${podId}:${channelId}`;
}

function getPodClient(podId: string) {
  const conn = usePodStore.getState().pods[podId];
  if (!conn?.podUrl || !conn?.pat) throw new Error("Not connected to pod");
  return createPodClient(conn.podUrl, conn.pat);
}

let nonceCounter = 0;
function generateNonce(): string {
  return `${Date.now()}-${++nonceCounter}`;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

interface MessagesState {
  /** composite key (podId:channelId) → ordered messages (oldest first) */
  byChannel: Record<string, ChannelMessages>;
  /** nonce → pending message (optimistic sends awaiting server confirmation) */
  pending: Record<string, PendingMessage>;
  /** messageId → reactions */
  reactions: Record<string, Reaction[]>;

  // Actions
  fetchMessages: (
    podId: string,
    channelId: string,
    opts?: { before?: string; after?: string },
  ) => Promise<void>;
  sendMessage: (
    podId: string,
    channelId: string,
    content: string,
    replyTo?: string | null,
  ) => Promise<void>;
  editMessage: (
    podId: string,
    channelId: string,
    messageId: string,
    content: string,
  ) => Promise<void>;
  deleteMessage: (
    podId: string,
    channelId: string,
    messageId: string,
  ) => Promise<void>;
  addReaction: (
    podId: string,
    channelId: string,
    messageId: string,
    emoji: string,
  ) => Promise<void>;
  removeReaction: (
    podId: string,
    channelId: string,
    messageId: string,
    emoji: string,
  ) => Promise<void>;

  // Gateway-driven mutations (called from handler.ts)
  gatewayMessageCreate: (podId: string, payload: MessagePayload) => void;
  gatewayMessageUpdate: (podId: string, payload: MessagePayload) => void;
  gatewayMessageDelete: (
    podId: string,
    channelId: string,
    messageId: string,
  ) => void;
  gatewayReactionAdd: (podId: string, payload: ReactionPayload) => void;
  gatewayReactionRemove: (podId: string, payload: ReactionPayload) => void;

  // Helpers
  clearChannel: (podId: string, channelId: string) => void;
  resetPod: (podId: string) => void;
  reset: () => void;
}

export const useMessageStore = create<MessagesState>()((set, get) => ({
  byChannel: {},
  pending: {},
  reactions: {},

  // ---------------------------------------------------------------------------
  // REST actions
  // ---------------------------------------------------------------------------

  fetchMessages: async (podId, channelId, opts) => {
    const key = channelKey(podId, channelId);
    const { before, after } = opts ?? {};
    const existing = get().byChannel[key];
    const isPaginating = before !== undefined || after !== undefined;

    // Skip re-fetching if we already have messages for this channel (initial load only)
    if (!isPaginating && existing && existing.messages.length > 0) return;

    // Skip if no more in requested direction
    if (before !== undefined && existing && !existing.hasOlder) return;
    if (after !== undefined && existing && !existing.hasNewer) return;

    // Mark loading
    set((state) => ({
      byChannel: {
        ...state.byChannel,
        [key]: {
          messages: existing?.messages ?? [],
          hasOlder: existing?.hasOlder ?? true,
          hasNewer: existing?.hasNewer ?? false,
          loading: true,
        },
      },
    }));

    try {
      const client = getPodClient(podId);
      const { data, error } = await client.GET(
        "/api/v1/channels/{channel_id}/messages",
        {
          params: {
            path: { channel_id: channelId },
            query: {
              limit: 50,
              ...(before !== undefined ? { before } : {}),
              ...(after !== undefined ? { after } : {}),
            },
          },
        },
      );

      if (error || !data) throw new Error("Failed to fetch messages");

      set((state) => {
        const prev = state.byChannel[key]?.messages ?? [];
        const prevState = state.byChannel[key];
        // Sort messages oldest-first by id
        const fetched = [...data.data].sort((a, b) => a.id.localeCompare(b.id));

        let merged: Message[];
        let hasOlder = prevState?.hasOlder ?? true;
        let hasNewer = prevState?.hasNewer ?? false;

        if (before !== undefined) {
          // Prepend older messages
          merged = [...fetched, ...prev];
          hasOlder = data.has_more;
        } else if (after !== undefined) {
          // Append newer messages
          merged = [...prev, ...fetched];
          hasNewer = data.has_more;
        } else {
          // Initial load — latest messages
          merged = fetched;
          hasOlder = data.has_more;
          // Initial load gets the latest, so no newer messages exist
          hasNewer = false;
        }

        return {
          byChannel: {
            ...state.byChannel,
            [key]: {
              messages: merged,
              hasOlder,
              hasNewer,
              loading: false,
            },
          },
        };
      });
    } catch {
      set((state) => ({
        byChannel: {
          ...state.byChannel,
          [key]: {
            messages: existing?.messages ?? [],
            hasOlder: existing?.hasOlder ?? true,
            hasNewer: existing?.hasNewer ?? false,
            loading: false,
          },
        },
      }));
    }
  },

  sendMessage: async (podId, channelId, content, replyTo) => {
    const nonce = generateNonce();
    const userId = usePodStore.getState().pods[podId]?.user?.id ?? "";

    // Optimistic insert
    const pendingMsg: PendingMessage = {
      nonce,
      podId,
      channel_id: channelId,
      content,
      reply_to: replyTo ?? null,
      created_at: new Date().toISOString(),
      author_id: userId,
    };
    set((state) => ({
      pending: { ...state.pending, [nonce]: pendingMsg },
    }));

    try {
      const client = getPodClient(podId);
      await client.POST("/api/v1/channels/{channel_id}/messages", {
        params: { path: { channel_id: channelId } },
        body: {
          content,
          reply_to: replyTo ?? null,
        },
      });

      // The gateway MESSAGE_CREATE event will reconcile the pending message.
      // Only clean up here if the gateway hasn't already done so.
      set((state) => {
        if (!state.pending[nonce]) return state;
        const next = { ...state.pending };
        delete next[nonce];
        return { pending: next };
      });
    } catch {
      // Remove the failed pending message
      set((state) => {
        const next = { ...state.pending };
        delete next[nonce];
        return { pending: next };
      });
    }
  },

  editMessage: async (podId, channelId, messageId, content) => {
    const key = channelKey(podId, channelId);
    try {
      const client = getPodClient(podId);
      const { data, error } = await client.PATCH(
        "/api/v1/channels/{channel_id}/messages/{message_id}",
        {
          params: {
            path: { channel_id: channelId, message_id: messageId },
          },
          body: { content },
        },
      );

      if (error || !data) throw new Error("Failed to edit message");

      // Optimistically update locally (gateway will also send MESSAGE_UPDATE)
      set((state) => {
        const channelMsgs = state.byChannel[key];
        if (!channelMsgs) return state;
        return {
          byChannel: {
            ...state.byChannel,
            [key]: {
              ...channelMsgs,
              messages: channelMsgs.messages.map((m) =>
                m.id === messageId ? { ...m, ...data } : m,
              ),
            },
          },
        };
      });
    } catch {
      // silently fail — message stays as-is
    }
  },

  deleteMessage: async (podId, channelId, messageId) => {
    const key = channelKey(podId, channelId);
    try {
      const client = getPodClient(podId);
      await client.DELETE(
        "/api/v1/channels/{channel_id}/messages/{message_id}",
        {
          params: {
            path: { channel_id: channelId, message_id: messageId },
          },
        },
      );

      // Optimistically remove (gateway will also send MESSAGE_DELETE)
      set((state) => {
        const channelMsgs = state.byChannel[key];
        if (!channelMsgs) return state;
        return {
          byChannel: {
            ...state.byChannel,
            [key]: {
              ...channelMsgs,
              messages: channelMsgs.messages.filter((m) => m.id !== messageId),
            },
          },
        };
      });
    } catch {
      // silently fail
    }
  },

  addReaction: async (podId, channelId, messageId, emoji) => {
    try {
      const client = getPodClient(podId);
      await client.PUT(
        "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
        {
          params: {
            path: { channel_id: channelId, message_id: messageId, emoji },
          },
        },
      );
    } catch {
      // silently fail
    }
  },

  removeReaction: async (podId, channelId, messageId, emoji) => {
    try {
      const client = getPodClient(podId);
      await client.DELETE(
        "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
        {
          params: {
            path: { channel_id: channelId, message_id: messageId, emoji },
          },
        },
      );
    } catch {
      // silently fail
    }
  },

  // ---------------------------------------------------------------------------
  // Gateway-driven mutations
  // ---------------------------------------------------------------------------

  gatewayMessageCreate: (podId, payload) => {
    const key = channelKey(podId, payload.channel_id);
    set((state) => {
      const channelMsgs = state.byChannel[key];

      // Convert gateway payload to Message shape
      const msg: Message = {
        id: payload.id,
        channel_id: payload.channel_id,
        author_id: payload.author_id,
        content: payload.content,
        type: payload.type,
        flags: payload.flags,
        reply_to: payload.reply_to,
        edited_at: payload.edited_at,
        pinned: payload.pinned,
        created_at: payload.created_at,
      };

      // If we don't have this channel loaded yet, skip (it'll be fetched on navigate)
      if (!channelMsgs) return state;

      // Deduplicate: if message ID already exists, don't add again
      if (channelMsgs.messages.some((m) => m.id === msg.id)) return state;

      // Reconcile pending messages: match by nonce or by content + author
      const nextPending = { ...state.pending };
      if (payload.nonce && nextPending[payload.nonce]) {
        delete nextPending[payload.nonce];
      } else {
        // No nonce — match by author + channel + content
        for (const [nKey, pm] of Object.entries(nextPending)) {
          if (
            pm.channel_id === payload.channel_id &&
            pm.author_id === payload.author_id &&
            pm.content === payload.content
          ) {
            delete nextPending[nKey];
            break;
          }
        }
      }

      return {
        byChannel: {
          ...state.byChannel,
          [key]: {
            ...channelMsgs,
            messages: [...channelMsgs.messages, msg],
          },
        },
        pending: nextPending,
      };
    });
  },

  gatewayMessageUpdate: (podId, payload) => {
    const key = channelKey(podId, payload.channel_id);
    set((state) => {
      const channelMsgs = state.byChannel[key];
      if (!channelMsgs) return state;

      return {
        byChannel: {
          ...state.byChannel,
          [key]: {
            ...channelMsgs,
            messages: channelMsgs.messages.map((m) =>
              m.id === payload.id
                ? {
                    ...m,
                    content: payload.content,
                    edited_at: payload.edited_at,
                    pinned: payload.pinned,
                    flags: payload.flags,
                  }
                : m,
            ),
          },
        },
      };
    });
  },

  gatewayMessageDelete: (podId, channelId, messageId) => {
    const key = channelKey(podId, channelId);
    set((state) => {
      const channelMsgs = state.byChannel[key];
      if (!channelMsgs) return state;

      return {
        byChannel: {
          ...state.byChannel,
          [key]: {
            ...channelMsgs,
            messages: channelMsgs.messages.filter((m) => m.id !== messageId),
          },
        },
      };
    });
  },

  gatewayReactionAdd: (podId, payload) => {
    set((state) => {
      const currentReactions = state.reactions[payload.message_id] ?? [];
      const existing = currentReactions.find((r) => r.emoji === payload.emoji);
      const myId = usePodStore.getState().pods[podId]?.user?.id;
      const isMe = payload.user_id === myId;

      const updated = existing
        ? currentReactions.map((r) =>
            r.emoji === payload.emoji
              ? { ...r, count: r.count + 1, me: r.me || isMe }
              : r,
          )
        : [...currentReactions, { emoji: payload.emoji, count: 1, me: isMe }];

      return {
        reactions: { ...state.reactions, [payload.message_id]: updated },
      };
    });
  },

  gatewayReactionRemove: (podId, payload) => {
    set((state) => {
      const currentReactions = state.reactions[payload.message_id] ?? [];
      const myId = usePodStore.getState().pods[podId]?.user?.id;
      const isMe = payload.user_id === myId;

      const updated = currentReactions
        .map((r) =>
          r.emoji === payload.emoji
            ? { ...r, count: r.count - 1, me: isMe ? false : r.me }
            : r,
        )
        .filter((r) => r.count > 0);

      return {
        reactions: { ...state.reactions, [payload.message_id]: updated },
      };
    });
  },

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  clearChannel: (podId, channelId) => {
    const key = channelKey(podId, channelId);
    set((state) => {
      const next = { ...state.byChannel };
      delete next[key];
      return { byChannel: next };
    });
  },

  resetPod: (podId) => {
    set((state) => {
      const prefix = `${podId}:`;
      const nextByChannel = { ...state.byChannel };
      for (const k of Object.keys(nextByChannel)) {
        if (k.startsWith(prefix)) delete nextByChannel[k];
      }
      // Also remove pending messages for this pod
      const nextPending = { ...state.pending };
      for (const [nKey, pm] of Object.entries(nextPending)) {
        if (pm.podId === podId) delete nextPending[nKey];
      }
      return { byChannel: nextByChannel, pending: nextPending };
    });
  },

  reset: () => set({ byChannel: {}, pending: {}, reactions: {} }),
}));
