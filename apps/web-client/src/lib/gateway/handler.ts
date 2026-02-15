/**
 * Dispatch event handler — routes gateway events to the appropriate Zustand stores.
 */

import type {
  DispatchEventName,
  DispatchPayloadMap,
  ReadyPayload,
  MessagePayload,
  MessageDeletePayload,
  ReactionPayload,
  ChannelPayload,
  ChannelDeletePayload,
  CommunityUpdatePayload,
  MemberPayload,
  MemberLeavePayload,
} from "@/types/gateway";
import {
  useCommunityStore,
  type Community,
  type Channel,
  type Role,
  type CommunityMember,
} from "@/stores/communities";
import { useMessageStore } from "@/stores/messages";
import { useTypingStore } from "@/stores/typing";
import { usePinStore } from "@/stores/pins";

/**
 * Route a dispatched event to the correct store update.
 *
 * Called from `GatewayConnection.onmessage` for every op=0 event
 * that isn't READY (READY is handled inline during connect).
 */
export function handleDispatch(
  event: DispatchEventName,
  data: unknown,
  podId: string,
): void {
  switch (event) {
    // --- Messages ---
    case "MESSAGE_CREATE":
      handleMessageCreate(podId, data as DispatchPayloadMap["MESSAGE_CREATE"]);
      break;
    case "MESSAGE_UPDATE":
      handleMessageUpdate(podId, data as DispatchPayloadMap["MESSAGE_UPDATE"]);
      break;
    case "MESSAGE_DELETE":
      handleMessageDelete(podId, data as DispatchPayloadMap["MESSAGE_DELETE"]);
      break;

    // --- Reactions ---
    case "MESSAGE_REACTION_ADD":
      handleReactionAdd(
        podId,
        data as DispatchPayloadMap["MESSAGE_REACTION_ADD"],
      );
      break;
    case "MESSAGE_REACTION_REMOVE":
      handleReactionRemove(
        podId,
        data as DispatchPayloadMap["MESSAGE_REACTION_REMOVE"],
      );
      break;

    // --- Channels ---
    case "CHANNEL_CREATE":
      handleChannelCreate(
        podId,
        data as DispatchPayloadMap["CHANNEL_CREATE"],
      );
      break;
    case "CHANNEL_UPDATE":
      handleChannelUpdate(
        podId,
        data as DispatchPayloadMap["CHANNEL_UPDATE"],
      );
      break;
    case "CHANNEL_DELETE":
      handleChannelDelete(
        podId,
        data as DispatchPayloadMap["CHANNEL_DELETE"],
      );
      break;

    // --- Communities ---
    case "COMMUNITY_UPDATE":
      handleCommunityUpdate(
        podId,
        data as DispatchPayloadMap["COMMUNITY_UPDATE"],
      );
      break;

    // --- Members ---
    case "MEMBER_JOIN":
      handleMemberJoin(podId, data as DispatchPayloadMap["MEMBER_JOIN"]);
      break;
    case "MEMBER_LEAVE":
      handleMemberLeave(podId, data as DispatchPayloadMap["MEMBER_LEAVE"]);
      break;
    case "MEMBER_UPDATE":
      handleMemberUpdate(podId, data as DispatchPayloadMap["MEMBER_UPDATE"]);
      break;

    // --- Typing ---
    case "TYPING_START":
      useTypingStore
        .getState()
        .gatewayTypingStart(
          podId,
          data as DispatchPayloadMap["TYPING_START"],
        );
      break;

    // --- Pins ---
    case "CHANNEL_PINS_UPDATE": {
      const pinsData = data as DispatchPayloadMap["CHANNEL_PINS_UPDATE"];
      usePinStore
        .getState()
        .gatewayChannelPinsUpdate(podId, pinsData.channel_id);
      break;
    }

    default:
      console.warn("[gateway] Unhandled dispatch event:", event);
  }
}

/**
 * Populate stores from the READY payload.
 * Called from the pod store after a successful IDENTIFY.
 */
export function handleReady(payload: ReadyPayload, podId: string): void {
  useCommunityStore.setState((state) => {
    const podCommunities = { ...(state.communities[podId] ?? {}) };
    const podChannels = { ...(state.channels[podId] ?? {}) };
    const podRoles = { ...(state.roles[podId] ?? {}) };

    for (const community of payload.communities) {
      // Merge community — READY data wins (has latest state)
      podCommunities[community.id] = {
        ...podCommunities[community.id],
        id: community.id,
        name: community.name,
        description: community.description,
        icon_url: community.icon_url,
        owner_id: community.owner_id,
        member_count: community.member_count,
      } as Community;

      // READY channels have message_count — always prefer them
      podChannels[community.id] = ([...community.channels] as Channel[]).sort(
        (a, b) => a.position - b.position,
      );
      podRoles[community.id] = ([...community.roles] as Role[]).sort(
        (a, b) => a.position - b.position,
      );
    }

    return {
      communities: { ...state.communities, [podId]: podCommunities },
      channels: { ...state.channels, [podId]: podChannels },
      roles: { ...state.roles, [podId]: podRoles },
      loading: false,
    };
  });
}

// ---------------------------------------------------------------------------
// Message handlers
// ---------------------------------------------------------------------------

function handleMessageCreate(podId: string, payload: MessagePayload) {
  useMessageStore.getState().gatewayMessageCreate(podId, payload);
}

function handleMessageUpdate(podId: string, payload: MessagePayload) {
  useMessageStore.getState().gatewayMessageUpdate(podId, payload);
}

function handleMessageDelete(podId: string, payload: MessageDeletePayload) {
  useMessageStore
    .getState()
    .gatewayMessageDelete(podId, payload.channel_id, payload.id);
}

// ---------------------------------------------------------------------------
// Reaction handlers
// ---------------------------------------------------------------------------

function handleReactionAdd(podId: string, payload: ReactionPayload) {
  useMessageStore.getState().gatewayReactionAdd(podId, payload);
}

function handleReactionRemove(podId: string, payload: ReactionPayload) {
  useMessageStore.getState().gatewayReactionRemove(podId, payload);
}

// ---------------------------------------------------------------------------
// Channel handlers
// ---------------------------------------------------------------------------

function handleChannelCreate(podId: string, payload: ChannelPayload) {
  useCommunityStore.setState((state) => {
    const podChannels: Record<string, Channel[]> =
      state.channels[podId] ?? {};
    const communityChannels = podChannels[payload.community_id] ?? [];
    const updated = [
      ...communityChannels,
      { ...payload, message_count: 0 },
    ].sort((a, b) => a.position - b.position);
    return {
      channels: {
        ...state.channels,
        [podId]: { ...podChannels, [payload.community_id]: updated },
      },
    };
  });
}

function handleChannelUpdate(podId: string, payload: ChannelPayload) {
  useCommunityStore.setState((state) => {
    const podChannels: Record<string, Channel[]> =
      state.channels[podId] ?? {};
    const communityChannels = podChannels[payload.community_id] ?? [];
    const updated = communityChannels
      .map((ch) => (ch.id === payload.id ? { ...ch, ...payload } : ch))
      .sort((a, b) => a.position - b.position);
    return {
      channels: {
        ...state.channels,
        [podId]: { ...podChannels, [payload.community_id]: updated },
      },
    };
  });
}

function handleChannelDelete(podId: string, payload: ChannelDeletePayload) {
  useCommunityStore.setState((state) => {
    const podChannels: Record<string, Channel[]> =
      state.channels[podId] ?? {};
    const communityChannels = podChannels[payload.community_id] ?? [];
    const updated = communityChannels.filter((ch) => ch.id !== payload.id);
    return {
      channels: {
        ...state.channels,
        [podId]: { ...podChannels, [payload.community_id]: updated },
      },
    };
  });
}

// ---------------------------------------------------------------------------
// Community handlers
// ---------------------------------------------------------------------------

function handleCommunityUpdate(
  podId: string,
  payload: CommunityUpdatePayload,
) {
  useCommunityStore.setState((state) => {
    const podCommunities = state.communities[podId] ?? {};
    const existing = podCommunities[payload.id];
    if (!existing) return state;
    return {
      communities: {
        ...state.communities,
        [podId]: {
          ...podCommunities,
          [payload.id]: { ...existing, ...payload },
        },
      },
    };
  });
}

// ---------------------------------------------------------------------------
// Member handlers
// ---------------------------------------------------------------------------

function handleMemberJoin(podId: string, payload: MemberPayload) {
  useCommunityStore.setState((state) => {
    const podMembers: Record<string, CommunityMember[]> =
      state.members[podId] ?? {};
    const existing = podMembers[payload.community_id] ?? [];
    // Avoid duplicates
    if (existing.some((m) => m.user_id === payload.user_id)) return state;
    const newMember = {
      community_id: payload.community_id,
      user_id: payload.user_id,
      nickname: payload.nickname,
      roles: payload.roles,
      joined_at: payload.joined_at,
    } as CommunityMember;
    return {
      members: {
        ...state.members,
        [podId]: {
          ...podMembers,
          [payload.community_id]: [...existing, newMember],
        },
      },
    };
  });

  // Bump member_count
  useCommunityStore.setState((state) => {
    const podCommunities = state.communities[podId] ?? {};
    const community = podCommunities[payload.community_id];
    if (!community) return state;
    return {
      communities: {
        ...state.communities,
        [podId]: {
          ...podCommunities,
          [payload.community_id]: {
            ...community,
            member_count: community.member_count + 1,
          },
        },
      },
    };
  });
}

function handleMemberLeave(podId: string, payload: MemberLeavePayload) {
  useCommunityStore.setState((state) => {
    const podMembers: Record<string, CommunityMember[]> =
      state.members[podId] ?? {};
    const existing = podMembers[payload.community_id] ?? [];
    return {
      members: {
        ...state.members,
        [podId]: {
          ...podMembers,
          [payload.community_id]: existing.filter(
            (m) => m.user_id !== payload.user_id,
          ),
        },
      },
    };
  });

  // Decrement member_count
  useCommunityStore.setState((state) => {
    const podCommunities = state.communities[podId] ?? {};
    const community = podCommunities[payload.community_id];
    if (!community) return state;
    return {
      communities: {
        ...state.communities,
        [podId]: {
          ...podCommunities,
          [payload.community_id]: {
            ...community,
            member_count: Math.max(0, community.member_count - 1),
          },
        },
      },
    };
  });
}

function handleMemberUpdate(podId: string, payload: MemberPayload) {
  useCommunityStore.setState((state) => {
    const podMembers: Record<string, CommunityMember[]> =
      state.members[podId] ?? {};
    const existing = podMembers[payload.community_id] ?? [];
    const updated = existing.map((m) =>
      m.user_id === payload.user_id ? { ...m, ...payload } : m,
    );
    return {
      members: {
        ...state.members,
        [podId]: { ...podMembers, [payload.community_id]: updated },
      },
    };
  });
}
