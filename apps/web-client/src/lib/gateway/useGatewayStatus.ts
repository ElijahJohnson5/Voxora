/**
 * React hook that subscribes to the gateway connection status.
 *
 * Uses `useSyncExternalStore` so React re-renders exactly when
 * the status changes (connected / connecting / reconnecting / disconnected).
 */

import { useSyncExternalStore } from "react";
import { gateway, type GatewayStatus } from "./connection";

function subscribe(onStoreChange: () => void): () => void {
  return gateway.onStatusChange(onStoreChange);
}

function getSnapshot(): GatewayStatus {
  return gateway.status;
}

export function useGatewayStatus(): GatewayStatus {
  return useSyncExternalStore(subscribe, getSnapshot);
}
