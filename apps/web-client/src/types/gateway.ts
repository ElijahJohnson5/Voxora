/**
 * Gateway WebSocket protocol types.
 *
 * Hand-written to match the Rust pod-api gateway (apps/pod-api/src/gateway/events.rs).
 * WebSocket protocol is not covered by OpenAPI, so these are the only manually
 * maintained types on the client.
 */

// ---------------------------------------------------------------------------
// Opcodes
// ---------------------------------------------------------------------------

export const Opcode = {
  /** Server → Client: dispatched event */
  DISPATCH: 0,
  /** Client → Server: heartbeat */
  HEARTBEAT: 1,
  /** Client → Server: identify with WS ticket */
  IDENTIFY: 2,
  /** Server → Client: heartbeat acknowledged */
  HEARTBEAT_ACK: 6,
} as const;

export type OpcodeValue = (typeof Opcode)[keyof typeof Opcode];

// ---------------------------------------------------------------------------
// Server → Client message
// ---------------------------------------------------------------------------

/** Any message received from the gateway. */
export interface GatewayMessage<T = unknown> {
  op: OpcodeValue;
  /** Event name — present only when op === DISPATCH */
  t?: DispatchEventName | null;
  /** Sequence number — present only when op === DISPATCH */
  s?: number | null;
  /** Event-specific payload */
  d: T;
}

// ---------------------------------------------------------------------------
// Client → Server messages
// ---------------------------------------------------------------------------

export interface IdentifyMessage {
  op: typeof Opcode.IDENTIFY;
  d: { ticket: string };
}

export interface HeartbeatMessage {
  op: typeof Opcode.HEARTBEAT;
  d: { seq: number };
}

export type ClientMessage = IdentifyMessage | HeartbeatMessage;

// ---------------------------------------------------------------------------
// Dispatch event names
// ---------------------------------------------------------------------------

export const DispatchEvent = {
  READY: "READY",
  MESSAGE_CREATE: "MESSAGE_CREATE",
  MESSAGE_UPDATE: "MESSAGE_UPDATE",
  MESSAGE_DELETE: "MESSAGE_DELETE",
  MESSAGE_REACTION_ADD: "MESSAGE_REACTION_ADD",
  MESSAGE_REACTION_REMOVE: "MESSAGE_REACTION_REMOVE",
  CHANNEL_CREATE: "CHANNEL_CREATE",
  CHANNEL_UPDATE: "CHANNEL_UPDATE",
  CHANNEL_DELETE: "CHANNEL_DELETE",
  COMMUNITY_UPDATE: "COMMUNITY_UPDATE",
  MEMBER_JOIN: "MEMBER_JOIN",
  MEMBER_LEAVE: "MEMBER_LEAVE",
  MEMBER_UPDATE: "MEMBER_UPDATE",
} as const;

export type DispatchEventName =
  (typeof DispatchEvent)[keyof typeof DispatchEvent];

// ---------------------------------------------------------------------------
// Dispatch event payloads
// ---------------------------------------------------------------------------

export interface GatewayUser {
  id: string;
  username: string;
  display_name: string;
  avatar_url: string | null;
}

export interface GatewayChannel {
  id: string;
  community_id: string;
  parent_id: string | null;
  name: string;
  topic: string | null;
  type: number;
  position: number;
  slowmode_seconds: number;
  nsfw: boolean;
  created_at: string;
  updated_at: string;
}

export interface GatewayRole {
  id: string;
  community_id: string;
  name: string;
  color: number | null;
  position: number;
  permissions: number;
  mentionable: boolean;
  is_default: boolean;
  created_at: string;
}

export interface GatewayCommunity {
  id: string;
  name: string;
  description: string | null;
  icon_url: string | null;
  owner_id: string;
  member_count: number;
  channels: GatewayChannel[];
  roles: GatewayRole[];
}

/** Sent after a successful IDENTIFY. */
export interface ReadyPayload {
  session_id: string;
  user: GatewayUser;
  communities: GatewayCommunity[];
  heartbeat_interval: number;
}

export interface HeartbeatAckPayload {
  ack: number;
}

// --- Message events ---

export interface MessagePayload {
  id: string;
  channel_id: string;
  author_id: string;
  content: string | null;
  type: number;
  flags: number;
  reply_to: string | null;
  edited_at: string | null;
  pinned: boolean;
  created_at: string;
  /** Author info inlined by the server */
  author?: GatewayUser;
  /** Nonce echoed back for optimistic reconciliation */
  nonce?: string | null;
}

export interface MessageDeletePayload {
  id: string;
  channel_id: string;
}

// --- Reaction events ---

export interface ReactionPayload {
  message_id: string;
  channel_id: string;
  user_id: string;
  emoji: string;
}

// --- Channel events ---

export type ChannelPayload = GatewayChannel;

export interface ChannelDeletePayload {
  id: string;
  community_id: string;
}

// --- Community events ---

export interface CommunityUpdatePayload {
  id: string;
  name?: string;
  description?: string | null;
  icon_url?: string | null;
}

// --- Member events ---

export interface MemberPayload {
  community_id: string;
  user_id: string;
  username: string;
  display_name: string;
  avatar_url: string | null;
  nickname: string | null;
  roles: string[];
  joined_at: string;
}

export interface MemberLeavePayload {
  community_id: string;
  user_id: string;
}

// ---------------------------------------------------------------------------
// Payload type map (for typed dispatch handlers)
// ---------------------------------------------------------------------------

export interface DispatchPayloadMap {
  READY: ReadyPayload;
  MESSAGE_CREATE: MessagePayload;
  MESSAGE_UPDATE: MessagePayload;
  MESSAGE_DELETE: MessageDeletePayload;
  MESSAGE_REACTION_ADD: ReactionPayload;
  MESSAGE_REACTION_REMOVE: ReactionPayload;
  CHANNEL_CREATE: ChannelPayload;
  CHANNEL_UPDATE: ChannelPayload;
  CHANNEL_DELETE: ChannelDeletePayload;
  COMMUNITY_UPDATE: CommunityUpdatePayload;
  MEMBER_JOIN: MemberPayload;
  MEMBER_LEAVE: MemberLeavePayload;
  MEMBER_UPDATE: MemberPayload;
}
