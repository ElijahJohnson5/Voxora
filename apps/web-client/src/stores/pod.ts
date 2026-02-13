import { create } from "zustand";
import { persist } from "zustand/middleware";
import { hubApi } from "@/lib/api/hub-client";
import { createPodClient } from "@/lib/api/pod-client";
import type { components } from "@/lib/api/pod";
import { useCommunityStore } from "@/stores/communities";
import { gateway } from "@/lib/gateway/connection";
import { handleReady } from "@/lib/gateway/handler";

export type PodUser = components["schemas"]["UserInfo"];

const MAX_RETRIES = 5;
const BASE_DELAY_MS = 1_000;

interface PodState {
  podId: string | null;
  podUrl: string | null;
  pat: string | null;
  refreshToken: string | null;
  wsTicket: string | null;
  wsUrl: string | null;
  user: PodUser | null;
  connected: boolean;
  connecting: boolean;
  error: string | null;

  connectToPod: (podId: string, podUrl: string) => Promise<void>;
  reconnect: () => Promise<void>;
  refreshPat: () => Promise<void>;
  disconnect: () => void;
  scheduleRefresh: (expiresIn: number) => void;
}

let refreshTimer: ReturnType<typeof setTimeout> | null = null;
let retryTimer: ReturnType<typeof setTimeout> | null = null;
let retryCount = 0;

function clearRetry() {
  if (retryTimer) {
    clearTimeout(retryTimer);
    retryTimer = null;
  }
  retryCount = 0;
}

function scheduleRetry(fn: () => void) {
  if (retryCount >= MAX_RETRIES) return false;
  const delay = BASE_DELAY_MS * Math.pow(2, retryCount);
  retryCount++;
  retryTimer = setTimeout(fn, delay);
  return true;
}

export const usePodStore = create<PodState>()(
  persist(
    (set, get) => ({
      podId: null,
      podUrl: null,
      pat: null,
      refreshToken: null,
      wsTicket: null,
      wsUrl: null,
      user: null,
      connected: false,
      connecting: false,
      error: null,

      connectToPod: async (podId: string, podUrl: string) => {
        set({ connecting: true, error: null });

        try {
          // 1. Get SIA from Hub
          const { data: siaData, error: siaError } = await hubApi.POST(
            "/api/v1/oidc/sia",
            { body: { pod_id: podId } },
          );

          if (siaError || !siaData) {
            throw new Error("Failed to get SIA from Hub");
          }

          // 2. Login to Pod with SIA
          const podClient = createPodClient(podUrl);
          const { data: loginData, error: loginError } = await podClient.POST(
            "/api/v1/auth/login",
            { body: { sia: siaData.sia } },
          );

          if (loginError || !loginData) {
            throw new Error("Failed to login to Pod");
          }

          // 3. Store results
          clearRetry();
          set({
            podId,
            podUrl,
            pat: loginData.access_token,
            refreshToken: loginData.refresh_token,
            wsTicket: loginData.ws_ticket,
            wsUrl: loginData.ws_url,
            user: loginData.user,
            connected: true,
            connecting: false,
            error: null,
          });

          // 4. Schedule PAT refresh
          get().scheduleRefresh(loginData.expires_in);

          // 5. Connect to Gateway WebSocket
          if (loginData.ws_url && loginData.ws_ticket) {
            try {
              const readyPayload = await gateway.connect(
                loginData.ws_url,
                loginData.ws_ticket,
              );
              handleReady(readyPayload);
            } catch (gwErr) {
              console.warn(
                "[pod] Gateway connection failed, falling back to REST",
                gwErr,
              );
              useCommunityStore.getState().fetchCommunities();
            }
          } else {
            // No WS ticket — fall back to REST
            useCommunityStore.getState().fetchCommunities();
          }
        } catch (err) {
          const message =
            err instanceof Error ? err.message : "Connection failed";
          const willRetry = scheduleRetry(() =>
            get().connectToPod(podId, podUrl),
          );
          set({
            connecting: willRetry,
            connected: false,
            error: willRetry
              ? `${message} — retrying (${retryCount}/${MAX_RETRIES})…`
              : message,
          });
        }
      },

      reconnect: async () => {
        const { podId, podUrl, refreshToken } = get();
        if (!podId || !podUrl || !refreshToken) return;

        set({ connecting: true, error: null });

        try {
          // Only request a new WS ticket if the gateway isn't already connected
          const needsGateway = gateway.status !== "connected";
          const podClient = createPodClient(podUrl);
          const { data, error } = await podClient.POST("/api/v1/auth/refresh", {
            body: {
              refresh_token: refreshToken,
              include_ws_ticket: needsGateway,
            },
          });

          if (error || !data) {
            // Refresh token expired — fall back to full SIA flow
            await get().connectToPod(podId, podUrl);
            return;
          }

          clearRetry();
          set({
            pat: data.access_token,
            refreshToken: data.refresh_token,
            wsTicket: data.ws_ticket ?? null,
            wsUrl: data.ws_url ?? null,
            connected: true,
            connecting: false,
          });

          get().scheduleRefresh(data.expires_in);

          // Connect to Gateway with the fresh ticket
          if (data.ws_url && data.ws_ticket) {
            try {
              const readyPayload = await gateway.connect(
                data.ws_url,
                data.ws_ticket,
              );
              handleReady(readyPayload);
            } catch (gwErr) {
              console.warn(
                "[pod] Gateway reconnect failed, falling back to REST",
                gwErr,
              );
              useCommunityStore.getState().fetchCommunities();
            }
          } else {
            useCommunityStore.getState().fetchCommunities();
          }
        } catch {
          // Pod unreachable — fall back to SIA with backoff
          await get().connectToPod(podId, podUrl);
        }
      },

      refreshPat: async () => {
        const { podUrl, refreshToken } = get();
        if (!podUrl || !refreshToken) return;

        try {
          const podClient = createPodClient(podUrl);
          const { data, error } = await podClient.POST("/api/v1/auth/refresh", {
            body: { refresh_token: refreshToken },
          });

          if (error || !data) {
            throw new Error("Failed to refresh PAT");
          }

          set({
            pat: data.access_token,
            refreshToken: data.refresh_token,
          });

          get().scheduleRefresh(data.expires_in);
        } catch {
          get().disconnect();
        }
      },

      disconnect: () => {
        clearRetry();
        if (refreshTimer) {
          clearTimeout(refreshTimer);
          refreshTimer = null;
        }
        gateway.disconnect();
        useCommunityStore.getState().reset();
        set({
          podId: null,
          podUrl: null,
          pat: null,
          refreshToken: null,
          wsTicket: null,
          wsUrl: null,
          user: null,
          connected: false,
          connecting: false,
          error: null,
        });
      },

      scheduleRefresh: (expiresIn: number) => {
        if (refreshTimer) {
          clearTimeout(refreshTimer);
          refreshTimer = null;
        }

        // Refresh 60 seconds before expiry
        const delay = Math.max(expiresIn * 1000 - 60_000, 0);

        refreshTimer = setTimeout(() => {
          get().refreshPat();
        }, delay);
      },
    }),
    {
      name: "voxora-pod",
      partialize: (state) => ({
        podId: state.podId,
        podUrl: state.podUrl,
        pat: state.pat,
        refreshToken: state.refreshToken,
        wsTicket: state.wsTicket,
        wsUrl: state.wsUrl,
        user: state.user,
      }),
    },
  ),
);

// On load, if we have persisted pod state, reconnect using refresh token (no SIA)
const initialPodState = usePodStore.getState();
if (
  initialPodState.podId &&
  initialPodState.podUrl &&
  initialPodState.refreshToken
) {
  usePodStore.getState().reconnect();
}
