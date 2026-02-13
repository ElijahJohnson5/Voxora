import createClient, { type Middleware } from "openapi-fetch";
import type { paths } from "./pod";

export function createPodClient(baseUrl: string, token?: string) {
  const client = createClient<paths>({ baseUrl });

  if (token) {
    const authMiddleware: Middleware = {
      onRequest({ request }) {
        request.headers.set("Authorization", `Bearer ${token}`);
        return request;
      },
    };
    client.use(authMiddleware);
  }

  return client;
}
