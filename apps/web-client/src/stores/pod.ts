import { create } from "zustand";
import { persist } from "zustand/middleware";
import { hubApi } from "@/lib/api/hub-client";
import { createPodClient } from "@/lib/api/pod-client";
import type { components } from "@/lib/api/pod";
import { useCommunityStore } from "@/stores/communities";
import { useMessageStore } from "@/stores/messages";
import { GatewayConnection } from "@/lib/gateway/connection";
import { handleReady } from "@/lib/gateway/handler";

export type PodUser = components["schemas"]["UserInfo"];

// ---------------------------------------------------------------------------
// Per-pod gateway instances (not serializable — stored outside zustand)
// ---------------------------------------------------------------------------

const gatewayMap = new Map<string, GatewayConnection>();

export function getGateway(podId: string): GatewayConnection | undefined {
  return gatewayMap.get(podId);
}

// ---------------------------------------------------------------------------
// Per-pod timers & retry state
// ---------------------------------------------------------------------------

const MAX_RETRIES = 5;
const BASE_DELAY_MS = 1_000;

const refreshTimers = new Map<string, ReturnType<typeof setTimeout>>();
const retryTimers = new Map<string, ReturnType<typeof setTimeout>>();
const retryCounts = new Map<string, number>();

function clearRetry(podId: string) {
  const timer = retryTimers.get(podId);
  if (timer) {
    clearTimeout(timer);
    retryTimers.delete(podId);
  }
  retryCounts.delete(podId);
}

function scheduleRetry(podId: string, fn: () => void): boolean {
  const count = retryCounts.get(podId) ?? 0;
  if (count >= MAX_RETRIES) return false;
  const delay = BASE_DELAY_MS * Math.pow(2, count);
  retryCounts.set(podId, count + 1);
  retryTimers.set(podId, setTimeout(fn, delay));
  return true;
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface PodConnectionData {
  podId: string;
  podUrl: string;
  podName: string;
  podIcon?: string;
  pat: string;
  refreshToken: string;
  wsTicket: string | null;
  wsUrl: string | null;
  user: PodUser;
  connected: boolean;
  connecting: boolean;
  error: string | null;
}

interface PodState {
  pods: Record<string, PodConnectionData>;
  activePodId: string | null;

  connectToPod: (
    podId: string,
    podUrl: string,
    podName: string,
    podIcon?: string,
  ) => Promise<void>;
  reconnect: (podId: string) => Promise<void>;
  refreshPat: (podId: string) => Promise<void>;
  disconnectFromPod: (podId: string) => void;
  disconnectAll: () => void;
  scheduleRefresh: (podId: string, expiresIn: number) => void;
  setActivePod: (podId: string) => void;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const usePodStore = create<PodState>()(
  persist(
    (set, get) => ({
      pods: {},
      activePodId: null,

      setActivePod: (podId) => set({ activePodId: podId }),

      connectToPod: async (podId, podUrl, podName, podIcon) => {
        // Set connecting state for this pod
        set((state) => ({
          pods: {
            ...state.pods,
            [podId]: {
              ...state.pods[podId],
              podId,
              podUrl,
              podName,
              podIcon,
              connected: false,
              connecting: true,
              error: null,
            },
          },
        }));

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
          clearRetry(podId);
          set((state) => ({
            pods: {
              ...state.pods,
              [podId]: {
                podId,
                podUrl,
                podName,
                podIcon,
                pat: loginData.access_token,
                refreshToken: loginData.refresh_token,
                wsTicket: loginData.ws_ticket,
                wsUrl: loginData.ws_url,
                user: loginData.user,
                connected: true,
                connecting: false,
                error: null,
              },
            },
          }));

          // 4. Schedule PAT refresh
          get().scheduleRefresh(podId, loginData.expires_in);

          // 5. Connect to Gateway WebSocket
          if (loginData.ws_url && loginData.ws_ticket) {
            const existingGw = gatewayMap.get(podId);
            if (existingGw) existingGw.disconnect();

            const gw = new GatewayConnection(podId);
            gatewayMap.set(podId, gw);

            gw.connect(loginData.ws_url, loginData.ws_ticket)
              .then((payload) => handleReady(payload, podId))
              .catch((gwErr) => {
                console.warn(
                  `[pod:${podId}] Gateway connection failed, REST data already loaded`,
                  gwErr,
                );
              });
          }
        } catch (err) {
          const message =
            err instanceof Error ? err.message : "Connection failed";
          const willRetry = scheduleRetry(podId, () =>
            get().connectToPod(podId, podUrl, podName, podIcon),
          );
          set((state) => ({
            pods: {
              ...state.pods,
              [podId]: {
                ...state.pods[podId],
                podId,
                podUrl,
                podName,
                podIcon,
                connecting: willRetry,
                connected: false,
                error: willRetry
                  ? `${message} — retrying (${retryCounts.get(podId) ?? 0}/${MAX_RETRIES})…`
                  : message,
              } as PodConnectionData,
            },
          }));
        }
      },

      reconnect: async (podId) => {
        const conn = get().pods[podId];
        if (!conn?.refreshToken) return;

        set((state) => ({
          pods: {
            ...state.pods,
            [podId]: { ...conn, connecting: true, error: null },
          },
        }));

        try {
          const existingGw = gatewayMap.get(podId);
          const needsGateway = !existingGw || existingGw.status !== "connected";
          const podClient = createPodClient(conn.podUrl);
          const { data, error } = await podClient.POST("/api/v1/auth/refresh", {
            body: {
              refresh_token: conn.refreshToken,
              include_ws_ticket: needsGateway,
            },
          });

          if (error || !data) {
            // Refresh token expired — fall back to full SIA flow
            await get().connectToPod(
              podId,
              conn.podUrl,
              conn.podName,
              conn.podIcon,
            );
            return;
          }

          clearRetry(podId);
          set((state) => ({
            pods: {
              ...state.pods,
              [podId]: {
                ...state.pods[podId],
                pat: data.access_token,
                refreshToken: data.refresh_token,
                wsTicket: data.ws_ticket ?? null,
                wsUrl: data.ws_url ?? null,
                connected: true,
                connecting: false,
              },
            },
          }));

          get().scheduleRefresh(podId, data.expires_in);

          // Fetch communities via REST immediately
          useCommunityStore.getState().fetchCommunities(podId);

          // Connect to Gateway in parallel
          if (data.ws_url && data.ws_ticket) {
            const oldGw = gatewayMap.get(podId);
            if (oldGw) oldGw.disconnect();

            const gw = new GatewayConnection(podId);
            gatewayMap.set(podId, gw);

            gw.connect(data.ws_url, data.ws_ticket)
              .then((payload) => handleReady(payload, podId))
              .catch((gwErr) => {
                console.warn(
                  `[pod:${podId}] Gateway reconnect failed, REST data already loaded`,
                  gwErr,
                );
              });
          }
        } catch {
          // Pod unreachable — fall back to SIA with backoff
          const conn = get().pods[podId];
          if (conn) {
            await get().connectToPod(
              podId,
              conn.podUrl,
              conn.podName,
              conn.podIcon,
            );
          }
        }
      },

      refreshPat: async (podId) => {
        const conn = get().pods[podId];
        if (!conn?.podUrl || !conn?.refreshToken) return;

        try {
          const podClient = createPodClient(conn.podUrl);
          const { data, error } = await podClient.POST("/api/v1/auth/refresh", {
            body: { refresh_token: conn.refreshToken },
          });

          if (error || !data) {
            throw new Error("Failed to refresh PAT");
          }

          set((state) => ({
            pods: {
              ...state.pods,
              [podId]: {
                ...state.pods[podId],
                pat: data.access_token,
                refreshToken: data.refresh_token,
              },
            },
          }));

          get().scheduleRefresh(podId, data.expires_in);
        } catch {
          get().disconnectFromPod(podId);
        }
      },

      disconnectFromPod: (podId) => {
        clearRetry(podId);
        const timer = refreshTimers.get(podId);
        if (timer) {
          clearTimeout(timer);
          refreshTimers.delete(podId);
        }
        const gw = gatewayMap.get(podId);
        if (gw) {
          gw.disconnect();
          gatewayMap.delete(podId);
        }
        useCommunityStore.getState().resetPod(podId);
        useMessageStore.getState().resetPod(podId);
        set((state) => {
          const next = { ...state.pods };
          delete next[podId];
          return {
            pods: next,
            activePodId: state.activePodId === podId ? null : state.activePodId,
          };
        });
      },

      disconnectAll: () => {
        for (const podId of Object.keys(get().pods)) {
          clearRetry(podId);
          const timer = refreshTimers.get(podId);
          if (timer) clearTimeout(timer);
          const gw = gatewayMap.get(podId);
          if (gw) gw.disconnect();
        }
        refreshTimers.clear();
        gatewayMap.clear();
        useCommunityStore.getState().reset();
        useMessageStore.getState().reset();
        set({ pods: {}, activePodId: null });
      },

      scheduleRefresh: (podId, expiresIn) => {
        const existing = refreshTimers.get(podId);
        if (existing) clearTimeout(existing);

        const delay = Math.max(expiresIn * 1000 - 60_000, 0);
        refreshTimers.set(
          podId,
          setTimeout(() => {
            get().refreshPat(podId);
          }, delay),
        );
      },
    }),
    {
      name: "voxora-pods",
      partialize: (state) => ({
        pods: Object.fromEntries(
          Object.entries(state.pods).map(([id, conn]) => [
            id,
            {
              podId: conn.podId,
              podUrl: conn.podUrl,
              podName: conn.podName,
              podIcon: conn.podIcon,
              pat: conn.pat,
              refreshToken: conn.refreshToken,
              wsTicket: conn.wsTicket,
              wsUrl: conn.wsUrl,
              user: conn.user,
            },
          ]),
        ),
        activePodId: state.activePodId,
      }),
      merge: (persisted, current) => {
        const persistedState = persisted as Partial<PodState>;
        const pods: Record<string, PodConnectionData> = {};

        if (persistedState.pods) {
          for (const [id, conn] of Object.entries(persistedState.pods)) {
            const c = conn as PodConnectionData;
            // Skip entries missing required fields (e.g. old store format)
            if (!c.podUrl || !c.refreshToken) continue;
            pods[id] = {
              ...c,
              podId: c.podId ?? id,
              podName: c.podName ?? id,
              connected: false,
              connecting: false,
              error: null,
            };
          }
        }

        return {
          ...(current as PodState),
          pods,
          activePodId: persistedState.activePodId ?? null,
        };
      },
    },
  ),
);

// On load, reconnect all persisted pods
const initialPods = usePodStore.getState().pods;
for (const podId of Object.keys(initialPods)) {
  const conn = initialPods[podId];
  if (conn?.refreshToken) {
    usePodStore.getState().reconnect(podId);
  }
}
