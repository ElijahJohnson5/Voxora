import { useEffect } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { usePodStore } from "@/stores/pod";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

const POD_ID = import.meta.env.VITE_POD_ID;
const POD_URL = import.meta.env.VITE_POD_URL;

export const Route = createFileRoute("/_authenticated/")({
  component: HomePage,
});

function HomePage() {
  const { connected, connecting, error, connectToPod, podId } = usePodStore();

  useEffect(() => {
    // Only trigger on fresh login (no persisted pod session, no prior error)
    // Reconnection from persisted state is handled by the store's on-load logic
    if (!connected && !connecting && !podId && POD_ID && POD_URL) {
      connectToPod(POD_ID, POD_URL);
    }
  }, [connected, connecting, podId, connectToPod]);

  if (connecting) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <Skeleton className="mx-auto mb-4 h-8 w-48" />
          <p className="text-muted-foreground">Connecting to pod...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h1 className="text-2xl font-bold">Connection Failed</h1>
          <p className="mt-2 text-muted-foreground">{error}</p>
          <Button
            className="mt-4"
            onClick={() => connectToPod(POD_ID, POD_URL)}
          >
            Retry
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center">
        <h1 className="text-2xl font-bold">Welcome to Voxora</h1>
        <p className="mt-2 text-muted-foreground">
          Select a community to get started
        </p>
      </div>
    </div>
  );
}
