import { create } from "zustand";
import { createPodClient } from "@/lib/api/pod-client";
import { usePodStore } from "@/stores/pod";
import type { components } from "@/lib/api/pod";

export type Community = components["schemas"]["Community"];
export type CommunityResponse = components["schemas"]["CommunityResponse"];
export type Channel = components["schemas"]["Channel"];
export type Role = components["schemas"]["Role"];
export type CommunityMember = components["schemas"]["CommunityMember"];

interface CommunitiesState {
  // podId → communityId → Community
  communities: Record<string, Record<string, Community>>;
  // podId → communityId → Channel[]
  channels: Record<string, Record<string, Channel[]>>;
  // podId → communityId → Role[]
  roles: Record<string, Record<string, Role[]>>;
  // podId → communityId → CommunityMember[]
  members: Record<string, Record<string, CommunityMember[]>>;
  loading: boolean;

  activePodId: string | null;
  activeCommunityId: string | null;
  activeChannelId: string | null;

  setActive: (podId: string, communityId: string) => void;
  setActiveChannel: (id: string) => void;
  fetchCommunities: (podId: string) => Promise<void>;
  fetchCommunity: (podId: string, communityId: string) => Promise<void>;
  fetchMembers: (podId: string, communityId: string) => Promise<void>;
  createCommunity: (
    podId: string,
    name: string,
    description?: string,
  ) => Promise<string>;
  joinViaInvite: (podId: string, code: string) => Promise<string>;
  resetPod: (podId: string) => void;
  reset: () => void;
}

function getPodClient(podId: string) {
  const conn = usePodStore.getState().pods[podId];
  if (!conn?.podUrl || !conn?.pat) throw new Error("Not connected to pod");
  return createPodClient(conn.podUrl, conn.pat);
}

export const useCommunityStore = create<CommunitiesState>()((set) => ({
  communities: {},
  channels: {},
  roles: {},
  members: {},
  loading: false,

  activePodId: null,
  activeCommunityId: null,
  activeChannelId: null,

  setActive: (podId, communityId) =>
    set({ activePodId: podId, activeCommunityId: communityId }),

  setActiveChannel: (id) => set({ activeChannelId: id }),

  fetchCommunity: async (podId, communityId) => {
    try {
      const client = getPodClient(podId);
      const { data, error } = await client.GET("/api/v1/communities/{id}", {
        params: { path: { id: communityId } },
      });
      if (error || !data) throw new Error("Failed to fetch community");

      const resp = data as CommunityResponse;
      set((state) => ({
        communities: {
          ...state.communities,
          [podId]: {
            ...(state.communities[podId] ?? {}),
            [resp.id]: resp,
          },
        },
        channels: {
          ...state.channels,
          [podId]: {
            ...(state.channels[podId] ?? {}),
            [resp.id]: [...resp.channels].sort(
              (a, b) => a.position - b.position,
            ),
          },
        },
        roles: {
          ...state.roles,
          [podId]: {
            ...(state.roles[podId] ?? {}),
            [resp.id]: [...resp.roles].sort(
              (a, b) => a.position - b.position,
            ),
          },
        },
      }));
    } catch {
      // silently fail — community just won't show up
    }
  },

  fetchCommunities: async (podId) => {
    set({ loading: true });
    try {
      const client = getPodClient(podId);

      // List all communities (returns Community[] without channels/roles)
      const { data: communityList, error: listError } = await client.GET(
        "/api/v1/communities",
      );
      if (listError || !communityList)
        throw new Error("Failed to fetch communities");

      const podCommunities: Record<string, Community> = {};

      for (const community of communityList) {
        podCommunities[community.id] = { ...community };
      }

      set((state) => ({
        communities: {
          ...state.communities,
          [podId]: podCommunities,
        },
        loading: false,
      }));
    } catch {
      set({ loading: false });
    }
  },

  fetchMembers: async (podId, communityId) => {
    try {
      const client = getPodClient(podId);
      const { data, error } = await client.GET(
        "/api/v1/communities/{community_id}/members",
        { params: { path: { community_id: communityId } } },
      );
      if (error || !data) throw new Error("Failed to fetch members");

      set((state) => ({
        members: {
          ...state.members,
          [podId]: {
            ...(state.members[podId] ?? {}),
            [communityId]: data.data,
          },
        },
      }));
    } catch {
      // silently fail — member list just stays empty
    }
  },

  createCommunity: async (podId, name, description) => {
    const client = getPodClient(podId);
    const { data, error } = await client.POST("/api/v1/communities", {
      body: { name, description: description || null },
    });
    if (error || !data) throw new Error("Failed to create community");

    const resp = data as CommunityResponse;
    set((state) => ({
      communities: {
        ...state.communities,
        [podId]: {
          ...(state.communities[podId] ?? {}),
          [resp.id]: resp,
        },
      },
      channels: {
        ...state.channels,
        [podId]: {
          ...(state.channels[podId] ?? {}),
          [resp.id]: [...resp.channels].sort(
            (a, b) => a.position - b.position,
          ),
        },
      },
      roles: {
        ...state.roles,
        [podId]: {
          ...(state.roles[podId] ?? {}),
          [resp.id]: [...resp.roles].sort((a, b) => a.position - b.position),
        },
      },
    }));

    return resp.id;
  },

  joinViaInvite: async (podId, code) => {
    const client = getPodClient(podId);
    const { data, error } = await client.POST("/api/v1/invites/{code}/accept", {
      params: { path: { code } },
    });
    if (error || !data) throw new Error("Failed to accept invite");

    const communityId = data.community_id;

    // Fetch the full community details
    const { data: communityData, error: communityError } = await client.GET(
      "/api/v1/communities/{id}",
      { params: { path: { id: communityId } } },
    );
    if (communityError || !communityData)
      throw new Error("Failed to fetch joined community");

    const resp = communityData as CommunityResponse;
    set((state) => ({
      communities: {
        ...state.communities,
        [podId]: {
          ...(state.communities[podId] ?? {}),
          [resp.id]: resp,
        },
      },
      channels: {
        ...state.channels,
        [podId]: {
          ...(state.channels[podId] ?? {}),
          [resp.id]: [...resp.channels].sort(
            (a, b) => a.position - b.position,
          ),
        },
      },
      roles: {
        ...state.roles,
        [podId]: {
          ...(state.roles[podId] ?? {}),
          [resp.id]: [...resp.roles].sort((a, b) => a.position - b.position),
        },
      },
    }));

    return communityId;
  },

  resetPod: (podId) =>
    set((state) => {
      const communities = { ...state.communities };
      const channels = { ...state.channels };
      const roles = { ...state.roles };
      const members = { ...state.members };
      delete communities[podId];
      delete channels[podId];
      delete roles[podId];
      delete members[podId];
      return { communities, channels, roles, members };
    }),

  reset: () =>
    set({
      communities: {},
      channels: {},
      roles: {},
      members: {},
      loading: false,
      activePodId: null,
      activeCommunityId: null,
      activeChannelId: null,
    }),
}));
