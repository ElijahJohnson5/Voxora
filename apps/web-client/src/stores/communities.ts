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
  communities: Record<string, Community>;
  channels: Record<string, Channel[]>;
  roles: Record<string, Role[]>;
  members: Record<string, CommunityMember[]>;
  loading: boolean;

  activeCommunityId: string | null;
  activeChannelId: string | null;

  setActiveCommunity: (id: string) => void;
  setActiveChannel: (id: string) => void;
  fetchCommunities: () => Promise<void>;
  fetchMembers: (communityId: string) => Promise<void>;
  createCommunity: (name: string, description?: string) => Promise<string>;
  joinViaInvite: (code: string) => Promise<string>;
  reset: () => void;
}

function getPodClient() {
  const { podUrl, pat } = usePodStore.getState();
  if (!podUrl || !pat) throw new Error("Not connected to pod");
  return createPodClient(podUrl, pat);
}

export const useCommunityStore = create<CommunitiesState>()((set) => ({
  communities: {},
  channels: {},
  roles: {},
  members: {},
  loading: false,

  activeCommunityId: null,
  activeChannelId: null,

  setActiveCommunity: (id) => set({ activeCommunityId: id }),
  setActiveChannel: (id) => set({ activeChannelId: id }),

  fetchCommunities: async () => {
    set({ loading: true });
    try {
      const client = getPodClient();

      // List all communities (returns Community[] without channels/roles)
      const { data: communityList, error: listError } = await client.GET(
        "/api/v1/communities",
      );
      if (listError || !communityList) throw new Error("Failed to fetch communities");

      // Fetch full details (with channels + roles) for each community
      const results = await Promise.all(
        communityList.map((c) =>
          client.GET("/api/v1/communities/{id}", { params: { path: { id: c.id } } }),
        ),
      );

      const communities: Record<string, Community> = {};
      const channels: Record<string, Channel[]> = {};
      const roles: Record<string, Role[]> = {};

      for (const { data, error } of results) {
        if (error || !data) continue;
        const resp = data as CommunityResponse;
        communities[resp.id] = resp;
        channels[resp.id] = [...resp.channels].sort((a, b) => a.position - b.position);
        roles[resp.id] = [...resp.roles].sort((a, b) => a.position - b.position);
      }

      set({ communities, channels, roles, loading: false });
    } catch {
      set({ loading: false });
    }
  },

  fetchMembers: async (communityId) => {
    try {
      const client = getPodClient();
      const { data, error } = await client.GET(
        "/api/v1/communities/{community_id}/members",
        { params: { path: { community_id: communityId } } },
      );
      if (error || !data) throw new Error("Failed to fetch members");

      set((state) => ({
        members: { ...state.members, [communityId]: data.data },
      }));
    } catch {
      // silently fail — member list just stays empty
    }
  },

  createCommunity: async (name, description) => {
    const client = getPodClient();
    const { data, error } = await client.POST("/api/v1/communities", {
      body: { name, description: description || null },
    });
    if (error || !data) throw new Error("Failed to create community");

    const resp = data as CommunityResponse;
    set((state) => ({
      communities: { ...state.communities, [resp.id]: resp },
      channels: {
        ...state.channels,
        [resp.id]: [...resp.channels].sort((a, b) => a.position - b.position),
      },
      roles: {
        ...state.roles,
        [resp.id]: [...resp.roles].sort((a, b) => a.position - b.position),
      },
    }));

    return resp.id;
  },

  joinViaInvite: async (code) => {
    const client = getPodClient();
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
      communities: { ...state.communities, [resp.id]: resp },
      channels: {
        ...state.channels,
        [resp.id]: [...resp.channels].sort((a, b) => a.position - b.position),
      },
      roles: {
        ...state.roles,
        [resp.id]: [...resp.roles].sort((a, b) => a.position - b.position),
      },
    }));

    return communityId;
  },

  reset: () =>
    set({
      communities: {},
      channels: {},
      roles: {},
      members: {},
      loading: false,
      activeCommunityId: null,
      activeChannelId: null,
    }),
}));
