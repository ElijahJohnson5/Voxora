import createClient from "openapi-fetch";
import type { paths } from "./hub";

export const hubApi = createClient<paths>({
  baseUrl: import.meta.env.VITE_HUB_URL || "http://localhost:4001",
});
