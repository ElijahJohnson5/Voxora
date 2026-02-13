import createClient from "openapi-fetch";
import type { paths } from "./pod";

export function createPodClient(baseUrl: string) {
  return createClient<paths>({ baseUrl });
}
