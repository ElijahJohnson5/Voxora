import { create } from "zustand";
import { persist } from "zustand/middleware";
import { refreshAccessToken, type TokenResult } from "@/lib/oidc";
import { setTokenGetter } from "@/lib/api/hub-client";

export interface User {
  id: string;
  username: string;
  displayName: string;
  email: string | null;
  avatarUrl: string | null;
}

interface AuthState {
  accessToken: string | null;
  refreshToken: string | null;
  idToken: string | null;
  expiresAt: number | null;
  user: User | null;

  setTokens: (result: TokenResult) => void;
  setUser: (user: User) => void;
  clearTokens: () => void;
  isAuthenticated: () => boolean;
  scheduleRefresh: () => void;
}

let refreshTimer: ReturnType<typeof setTimeout> | null = null;

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      accessToken: null,
      refreshToken: null,
      idToken: null,
      expiresAt: null,
      user: null,

      setTokens: (result: TokenResult) => {
        const expiresAt = Date.now() + result.expiresIn * 1000;
        set({
          accessToken: result.accessToken,
          refreshToken: result.refreshToken ?? get().refreshToken,
          idToken: result.idToken,
          expiresAt,
        });
        get().scheduleRefresh();
      },

      setUser: (user: User) => {
        set({ user });
      },

      clearTokens: () => {
        if (refreshTimer) {
          clearTimeout(refreshTimer);
          refreshTimer = null;
        }
        set({
          accessToken: null,
          refreshToken: null,
          idToken: null,
          expiresAt: null,
          user: null,
        });
      },

      isAuthenticated: () => {
        const { accessToken, expiresAt } = get();
        return !!accessToken && !!expiresAt && Date.now() < expiresAt;
      },

      scheduleRefresh: () => {
        if (refreshTimer) {
          clearTimeout(refreshTimer);
          refreshTimer = null;
        }

        const { expiresAt, refreshToken } = get();
        if (!expiresAt || !refreshToken) return;

        // Refresh 60 seconds before expiry
        const delay = Math.max(expiresAt - Date.now() - 60_000, 0);

        refreshTimer = setTimeout(async () => {
          const currentRefresh = get().refreshToken;
          if (!currentRefresh) return;

          try {
            const result = await refreshAccessToken(currentRefresh);
            get().setTokens(result);
          } catch {
            get().clearTokens();
          }
        }, delay);
      },
    }),
    {
      name: "voxora-auth",
      partialize: (state) => ({
        accessToken: state.accessToken,
        refreshToken: state.refreshToken,
        idToken: state.idToken,
        expiresAt: state.expiresAt,
        user: state.user,
      }),
    },
  ),
);

// Wire the token getter for the Hub API client
setTokenGetter(() => useAuthStore.getState().accessToken);

// Resume refresh timer on load if we have valid tokens
const initialState = useAuthStore.getState();
if (initialState.isAuthenticated()) {
  initialState.scheduleRefresh();
}
