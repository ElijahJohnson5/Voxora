import { useEffect, useState } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { toast } from "sonner";
import { usePodStore } from "@/stores/pod";
import { useCommunityStore } from "@/stores/communities";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  CreateCommunityDialog,
  JoinInviteDialog,
} from "@/components/communities/community-dialogs";

const POD_ID = import.meta.env.VITE_POD_ID;
const POD_URL = import.meta.env.VITE_POD_URL;

export const Route = createFileRoute("/_authenticated/")({
  component: HomePage,
});

function HomePage() {
  const navigate = useNavigate();
  const { connected, connecting, error, connectToPod, podId } = usePodStore();
  const { communities, channels, loading, createCommunity, joinViaInvite } =
    useCommunityStore();
  const [createOpen, setCreateOpen] = useState(false);
  const [joinOpen, setJoinOpen] = useState(false);

  useEffect(() => {
    // Only trigger on fresh login (no persisted pod session, no prior error)
    // Reconnection from persisted state is handled by the store's on-load logic
    if (!connected && !connecting && !podId && POD_ID && POD_URL) {
      connectToPod(POD_ID, POD_URL);
    }
  }, [connected, connecting, podId, connectToPod]);

  const communityList = Object.values(communities);

  // Auto-navigate to first community's first channel once loaded
  useEffect(() => {
    if (!connected || loading) return;

    if (communityList.length === 0) return;

    const firstCommunity = communityList[0];
    const communityChannels = channels[firstCommunity.id] ?? [];
    const targetChannel =
      firstCommunity.default_channel ?? communityChannels[0]?.id;

    if (targetChannel) {
      navigate({
        to: "/community/$communityId/channel/$channelId",
        params: { communityId: firstCommunity.id, channelId: targetChannel },
        replace: true,
      });
    }
  }, [connected, loading, communities, channels, navigate]);

  if (connecting || (connected && loading)) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <Skeleton className="mx-auto mb-4 h-8 w-48" />
          <p className="text-muted-foreground">
            {connecting ? "Connecting to pod..." : "Loading communities..."}
          </p>
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

  if (connected && !loading && communityList.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="w-full max-w-md rounded-lg border border-border bg-background p-6 text-center shadow-sm">
          <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-secondary text-lg font-semibold text-secondary-foreground">
            P
          </div>
          <h1 className="text-2xl font-bold">Create your first community</h1>
          <p className="mt-2 text-muted-foreground">
            Start a new space or join with an invite code to get chatting.
          </p>
          <div className="mt-6 flex flex-col gap-2 sm:flex-row sm:justify-center">
            <Button onClick={() => setCreateOpen(true)}>
              Create community
            </Button>
            <Button variant="outline" onClick={() => setJoinOpen(true)}>
              Join via invite
            </Button>
          </div>
        </div>

        <CreateCommunityDialog
          open={createOpen}
          onOpenChange={setCreateOpen}
          onCreate={async (name, description) => {
            try {
              const id = await createCommunity(name, description);
              toast.success("Community created");
              setCreateOpen(false);
              const communityChannels = channels[id] ?? [];
              const community = communities[id];
              const defaultChannel =
                community?.default_channel ?? communityChannels[0]?.id;
              if (defaultChannel) {
                navigate({
                  to: "/community/$communityId/channel/$channelId",
                  params: { communityId: id, channelId: defaultChannel },
                  replace: true,
                });
              }
            } catch (err) {
              toast.error(
                err instanceof Error
                  ? err.message
                  : "Failed to create community",
              );
            }
          }}
        />
        <JoinInviteDialog
          open={joinOpen}
          onOpenChange={setJoinOpen}
          onJoin={async (code) => {
            try {
              const id = await joinViaInvite(code);
              toast.success("Joined community");
              setJoinOpen(false);
              const communityChannels = channels[id] ?? [];
              const community = communities[id];
              const defaultChannel =
                community?.default_channel ?? communityChannels[0]?.id;
              if (defaultChannel) {
                navigate({
                  to: "/community/$communityId/channel/$channelId",
                  params: { communityId: id, channelId: defaultChannel },
                  replace: true,
                });
              }
            } catch (err) {
              toast.error(
                err instanceof Error ? err.message : "Failed to join community",
              );
            }
          }}
        />
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
