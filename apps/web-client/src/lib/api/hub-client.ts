import createClient, { type Middleware } from "openapi-fetch";
import type { paths } from "./hub";

let tokenGetter: (() => string | null) | null = null;

/** Register a function that returns the current access token. Called by the auth store on init. */
export function setTokenGetter(getter: () => string | null) {
  tokenGetter = getter;
}

const authMiddleware: Middleware = {
  onRequest({ request }) {
    const token = tokenGetter?.();
    if (token) {
      request.headers.set("Authorization", `Bearer ${token}`);
    }
    return request;
  },
};

export const hubApi = createClient<paths>({
  baseUrl: import.meta.env.VITE_HUB_URL || "http://localhost:4001",
});

hubApi.use(authMiddleware);
