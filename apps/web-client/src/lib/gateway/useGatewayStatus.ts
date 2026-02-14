/**
 * React hook that subscribes to a pod's gateway connection status.
 *
 * Uses `useSyncExternalStore` so React re-renders exactly when
 * the status changes (connected / connecting / reconnecting / disconnected).
 */

import { useCallback, useSyncExternalStore } from "react";
import { type GatewayStatus } from "./connection";
import { getGateway } from "@/stores/pod";

export function useGatewayStatus(
  podId?: string | null,
): GatewayStatus {
  const subscribe = useCallback(
    (onStoreChange: () => void): (() => void) => {
      const gw = podId ? getGateway(podId) : null;
      if (!gw) return () => undefined;
      return gw.onStatusChange(onStoreChange);
    },
    [podId],
  );

  const getSnapshot = useCallback((): GatewayStatus => {
    const gw = podId ? getGateway(podId) : null;
    return gw?.status ?? "disconnected";
  }, [podId]);

  return useSyncExternalStore(subscribe, getSnapshot);
}
