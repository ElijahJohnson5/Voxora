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
} from "@/stores/communities";

/**
 * Route a dispatched event to the correct store update.
 *
 * Called from `GatewayConnection.onmessage` for every op=0 event
 * that isn't READY (READY is handled inline during connect).
 */
export function handleDispatch(event: DispatchEventName, data: unknown): void {
  switch (event) {
    // --- Messages ---
    case "MESSAGE_CREATE":
      handleMessageCreate(data as DispatchPayloadMap["MESSAGE_CREATE"]);
      break;
    case "MESSAGE_UPDATE":
      handleMessageUpdate(data as DispatchPayloadMap["MESSAGE_UPDATE"]);
      break;
    case "MESSAGE_DELETE":
      handleMessageDelete(data as DispatchPayloadMap["MESSAGE_DELETE"]);
      break;

    // --- Reactions ---
    case "MESSAGE_REACTION_ADD":
      handleReactionAdd(data as DispatchPayloadMap["MESSAGE_REACTION_ADD"]);
      break;
    case "MESSAGE_REACTION_REMOVE":
      handleReactionRemove(
        data as DispatchPayloadMap["MESSAGE_REACTION_REMOVE"],
      );
      break;

    // --- Channels ---
    case "CHANNEL_CREATE":
      handleChannelCreate(data as DispatchPayloadMap["CHANNEL_CREATE"]);
      break;
    case "CHANNEL_UPDATE":
      handleChannelUpdate(data as DispatchPayloadMap["CHANNEL_UPDATE"]);
      break;
    case "CHANNEL_DELETE":
      handleChannelDelete(data as DispatchPayloadMap["CHANNEL_DELETE"]);
      break;

    // --- Communities ---
    case "COMMUNITY_UPDATE":
      handleCommunityUpdate(data as DispatchPayloadMap["COMMUNITY_UPDATE"]);
      break;

    // --- Members ---
    case "MEMBER_JOIN":
      handleMemberJoin(data as DispatchPayloadMap["MEMBER_JOIN"]);
      break;
    case "MEMBER_LEAVE":
      handleMemberLeave(data as DispatchPayloadMap["MEMBER_LEAVE"]);
      break;
    case "MEMBER_UPDATE":
      handleMemberUpdate(data as DispatchPayloadMap["MEMBER_UPDATE"]);
      break;

    default:
      console.warn("[gateway] Unhandled dispatch event:", event);
  }
}

/**
 * Populate stores from the READY payload.
 * Called from `GatewayConnection` after a successful IDENTIFY.
 */
export function handleReady(payload: ReadyPayload): void {
  const communities: Record<string, Community> = {};
  const channels: Record<string, Channel[]> = {};
  const roles: Record<string, Role[]> = {};

  for (const community of payload.communities) {
    communities[community.id] = {
      id: community.id,
      name: community.name,
      description: community.description,
      icon_url: community.icon_url,
      owner_id: community.owner_id,
      member_count: community.member_count,
    } as Community;
    channels[community.id] = ([...community.channels] as Channel[]).sort(
      (a, b) => a.position - b.position,
    );
    roles[community.id] = ([...community.roles] as Role[]).sort(
      (a, b) => a.position - b.position,
    );
  }

  useCommunityStore.setState({
    communities,
    channels,
    roles,
    loading: false,
  });
}

// ---------------------------------------------------------------------------
// Message handlers
// ---------------------------------------------------------------------------

// Messages store doesn't exist yet (Task C-6). These handlers are stubs that
// will be wired to the messages store once it's created. For now they log.

function handleMessageCreate(payload: MessagePayload) {
  console.debug("[gateway] MESSAGE_CREATE", payload.id, payload.channel_id);
  // TODO (C-6): messageStore.addMessage(payload)
}

function handleMessageUpdate(payload: MessagePayload) {
  console.debug("[gateway] MESSAGE_UPDATE", payload.id);
  // TODO (C-6): messageStore.updateMessage(payload)
}

function handleMessageDelete(payload: MessageDeletePayload) {
  console.debug("[gateway] MESSAGE_DELETE", payload.id);
  // TODO (C-6): messageStore.deleteMessage(payload.channel_id, payload.id)
}

// ---------------------------------------------------------------------------
// Reaction handlers
// ---------------------------------------------------------------------------

function handleReactionAdd(payload: ReactionPayload) {
  console.debug(
    "[gateway] MESSAGE_REACTION_ADD",
    payload.message_id,
    payload.emoji,
  );
  // TODO (C-6): messageStore.addReaction(payload)
}

function handleReactionRemove(payload: ReactionPayload) {
  console.debug(
    "[gateway] MESSAGE_REACTION_REMOVE",
    payload.message_id,
    payload.emoji,
  );
  // TODO (C-6): messageStore.removeReaction(payload)
}

// ---------------------------------------------------------------------------
// Channel handlers
// ---------------------------------------------------------------------------

function handleChannelCreate(payload: ChannelPayload) {
  useCommunityStore.setState((state) => {
    const communityChannels = state.channels[payload.community_id] ?? [];
    const updated = [...communityChannels, payload].sort(
      (a, b) => a.position - b.position,
    );
    return {
      channels: { ...state.channels, [payload.community_id]: updated },
    };
  });
}

function handleChannelUpdate(payload: ChannelPayload) {
  useCommunityStore.setState((state) => {
    const communityChannels = state.channels[payload.community_id] ?? [];
    const updated = communityChannels
      .map((ch) => (ch.id === payload.id ? { ...ch, ...payload } : ch))
      .sort((a, b) => a.position - b.position);
    return {
      channels: { ...state.channels, [payload.community_id]: updated },
    };
  });
}

function handleChannelDelete(payload: ChannelDeletePayload) {
  useCommunityStore.setState((state) => {
    const communityChannels = state.channels[payload.community_id] ?? [];
    const updated = communityChannels.filter((ch) => ch.id !== payload.id);
    return {
      channels: { ...state.channels, [payload.community_id]: updated },
    };
  });
}

// ---------------------------------------------------------------------------
// Community handlers
// ---------------------------------------------------------------------------

function handleCommunityUpdate(payload: CommunityUpdatePayload) {
  useCommunityStore.setState((state) => {
    const existing = state.communities[payload.id];
    if (!existing) return state;
    return {
      communities: {
        ...state.communities,
        [payload.id]: { ...existing, ...payload },
      },
    };
  });
}

// ---------------------------------------------------------------------------
// Member handlers
// ---------------------------------------------------------------------------

function handleMemberJoin(payload: MemberPayload) {
  useCommunityStore.setState((state) => {
    const existing = state.members[payload.community_id] ?? [];
    // Avoid duplicates
    if (existing.some((m) => m.user_id === payload.user_id)) return state;
    const newMember = {
      community_id: payload.community_id,
      user_id: payload.user_id,
      nickname: payload.nickname,
      roles: payload.roles,
      joined_at: payload.joined_at,
    };
    return {
      members: {
        ...state.members,
        [payload.community_id]: [...existing, newMember],
      },
    };
  });

  // Bump member_count
  useCommunityStore.setState((state) => {
    const community = state.communities[payload.community_id];
    if (!community) return state;
    return {
      communities: {
        ...state.communities,
        [payload.community_id]: {
          ...community,
          member_count: community.member_count + 1,
        },
      },
    };
  });
}

function handleMemberLeave(payload: MemberLeavePayload) {
  useCommunityStore.setState((state) => {
    const existing = state.members[payload.community_id] ?? [];
    return {
      members: {
        ...state.members,
        [payload.community_id]: existing.filter(
          (m) => m.user_id !== payload.user_id,
        ),
      },
    };
  });

  // Decrement member_count
  useCommunityStore.setState((state) => {
    const community = state.communities[payload.community_id];
    if (!community) return state;
    return {
      communities: {
        ...state.communities,
        [payload.community_id]: {
          ...community,
          member_count: Math.max(0, community.member_count - 1),
        },
      },
    };
  });
}

function handleMemberUpdate(payload: MemberPayload) {
  useCommunityStore.setState((state) => {
    const existing = state.members[payload.community_id] ?? [];
    const updated = existing.map((m) =>
      m.user_id === payload.user_id ? { ...m, ...payload } : m,
    );
    return {
      members: {
        ...state.members,
        [payload.community_id]: updated,
      },
    };
  });
}
