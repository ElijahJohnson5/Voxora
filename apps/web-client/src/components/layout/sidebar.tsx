import { useState } from "react";
import { useMatch, useNavigate } from "@tanstack/react-router";
import { Plus, ArrowDownToLine, Home } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { useAuthStore } from "@/stores/auth";
import { usePodStore } from "@/stores/pod";
import { useCommunityStore } from "@/stores/communities";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  CreateCommunityDialog,
  JoinInviteDialog,
} from "@/components/communities/community-dialogs";

function getInitials(name: string) {
  return name
    .split(/\s+/)
    .map((w) => w[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
}

export function Sidebar() {
  const navigate = useNavigate();
  const user = useAuthStore((s) => s.user);
  const pods = usePodStore((s) => s.pods);
  const communities = useCommunityStore((s) => s.communities);
  const channels = useCommunityStore((s) => s.channels);
  const createCommunity = useCommunityStore((s) => s.createCommunity);
  const joinViaInvite = useCommunityStore((s) => s.joinViaInvite);

  // Read active IDs from URL — always in sync, no flash
  const communityMatch = useMatch({
    from: "/_authenticated/pod/$podId/community/$communityId",
    shouldThrow: false,
  });
  const settingsMatch = useMatch({
    from: "/_authenticated/settings",
    shouldThrow: false,
  });
  const activePodId = communityMatch?.params.podId ?? null;
  const activeCommunityId = communityMatch?.params.communityId ?? null;

  const channelMatch = useMatch({
    from: "/_authenticated/pod/$podId/community/$communityId/channel/$channelId",
    shouldThrow: false,
  });
  const activeChannelId = channelMatch?.params.channelId ?? null;

  const [createOpen, setCreateOpen] = useState(false);
  const [joinOpen, setJoinOpen] = useState(false);

  const connectedPods = Object.values(pods).filter((p) => p.connected);

  const activeCommunity =
    activePodId && activeCommunityId
      ? communities[activePodId]?.[activeCommunityId]
      : null;
  const activeChannels =
    activePodId && activeCommunityId
      ? (channels[activePodId]?.[activeCommunityId] ?? [])
      : [];

  function navigateToCommunity(podId: string, communityId: string) {
    const communityChannels = channels[podId]?.[communityId] ?? [];
    const community = communities[podId]?.[communityId];
    const defaultChannel =
      community?.default_channel ?? communityChannels[0]?.id;
    if (defaultChannel) {
      navigate({
        to: "/pod/$podId/community/$communityId/channel/$channelId",
        params: { podId, communityId, channelId: defaultChannel },
      });
    }
  }

  return (
    <div
      className={cn(
        "flex h-full shrink-0 border-r border-border",
        settingsMatch || !activeCommunityId ? "w-16" : "w-60",
      )}
    >
      {/* Community icon strip grouped by pod */}
      <TooltipProvider delayDuration={100}>
        <div className="flex w-16 flex-col items-center gap-2 border-r border-border bg-secondary/50">
          <ScrollArea className="flex-1 w-full">
            <div className="flex flex-col items-center gap-1 py-3">
              {/* Home button */}
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    onClick={() => navigate({ to: "/" })}
                    className="mb-1 flex h-10 w-10 items-center justify-center rounded-full bg-muted text-muted-foreground transition-colors hover:bg-primary hover:text-primary-foreground"
                  >
                    <Home className="h-5 w-5" />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="right">Pod Browser</TooltipContent>
              </Tooltip>

              <Separator className="w-8 my-1" />

              {connectedPods.map((pod) => {
                const podCommunities = Object.values(
                  communities[pod.podId] ?? {},
                );
                return (
                  <div
                    key={pod.podId}
                    className="flex flex-col items-center gap-1 w-full"
                  >
                    {/* Pod header */}
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <div className="flex h-6 w-full items-center justify-center">
                          <span className="truncate px-1 text-[9px] font-semibold uppercase text-muted-foreground">
                            {(pod.podName ?? pod.podId).slice(0, 6)}
                          </span>
                        </div>
                      </TooltipTrigger>
                      <TooltipContent side="right">
                        {pod.podName ?? pod.podId}
                      </TooltipContent>
                    </Tooltip>

                    {/* Communities for this pod */}
                    {podCommunities.map((community) => (
                      <Tooltip key={community.id}>
                        <TooltipTrigger asChild>
                          <button
                            onClick={() =>
                              navigateToCommunity(pod.podId, community.id)
                            }
                            className="flex items-center justify-center"
                          >
                            <Avatar
                              className={cn(
                                "h-10 w-10 cursor-pointer transition-all",
                                activePodId === pod.podId &&
                                  activeCommunityId === community.id &&
                                  "ring-2 ring-primary",
                              )}
                            >
                              {community.icon_url && (
                                <AvatarImage
                                  src={community.icon_url}
                                  alt={community.name}
                                />
                              )}
                              <AvatarFallback className="text-xs font-medium">
                                {getInitials(community.name)}
                              </AvatarFallback>
                            </Avatar>
                          </button>
                        </TooltipTrigger>
                        <TooltipContent side="right">
                          {community.name}
                        </TooltipContent>
                      </Tooltip>
                    ))}

                    <Separator className="w-8 my-1" />
                  </div>
                );
              })}
            </div>
          </ScrollArea>

          <Separator className="w-8" />

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => setCreateOpen(true)}
                className="flex h-10 w-10 items-center justify-center rounded-full bg-muted text-muted-foreground transition-colors hover:bg-primary hover:text-primary-foreground"
              >
                <Plus className="h-5 w-5" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">Create Community</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => setJoinOpen(true)}
                className="flex h-10 w-10 items-center justify-center rounded-full bg-muted text-muted-foreground transition-colors hover:bg-primary hover:text-primary-foreground"
              >
                <ArrowDownToLine className="h-5 w-5" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">Join via Invite</TooltipContent>
          </Tooltip>

          <Separator className="w-8" />

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => navigate({ to: "/settings" })}
                className="mb-3 flex items-center justify-center"
              >
                <Avatar
                  className={cn(
                    "h-10 w-10 cursor-pointer transition-all hover:ring-2 hover:ring-primary",
                    settingsMatch && "ring-2 ring-primary",
                  )}
                >
                  {user?.avatarUrl && (
                    <AvatarImage
                      src={user.avatarUrl}
                      alt={user.displayName}
                    />
                  )}
                  <AvatarFallback className="text-xs font-medium">
                    {user ? getInitials(user.displayName) : "?"}
                  </AvatarFallback>
                </Avatar>
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">Settings</TooltipContent>
          </Tooltip>
        </div>
      </TooltipProvider>

      {/* Channel list — only shown when a community is active */}
      {!settingsMatch && activeCommunityId && (
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="flex h-12 items-center border-b border-border px-3">
            <h2 className="truncate text-sm font-semibold">
              {activeCommunity?.name ?? "Select a community"}
            </h2>
          </div>
          <ScrollArea className="flex-1">
            <nav className="space-y-0.5 px-2 py-2">
              {activeChannels.map((channel) => (
                <Button
                  key={channel.id}
                  variant="ghost"
                  className={cn(
                    "w-full justify-start gap-2 px-2",
                    activeChannelId === channel.id && "bg-accent",
                  )}
                  onClick={() => {
                    if (!activePodId || !activeCommunityId) return;
                    navigate({
                      to: "/pod/$podId/community/$communityId/channel/$channelId",
                      params: {
                        podId: activePodId,
                        communityId: activeCommunityId,
                        channelId: channel.id,
                      },
                    });
                  }}
                >
                  <span className="text-xs text-muted-foreground">#</span>
                  {channel.name}
                </Button>
              ))}
            </nav>
          </ScrollArea>
        </div>
      )}

      <CreateCommunityDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        pods={connectedPods.map((p) => ({ podId: p.podId, podName: p.podName ?? p.podId }))}
        onCreate={async (podId, name, description) => {
          try {
            const id = await createCommunity(podId, name, description);
            toast.success("Community created");
            setCreateOpen(false);
            navigateToCommunity(podId, id);
          } catch (err) {
            toast.error(
              err instanceof Error ? err.message : "Failed to create community",
            );
          }
        }}
      />
      <JoinInviteDialog
        open={joinOpen}
        onOpenChange={setJoinOpen}
        pods={connectedPods.map((p) => ({ podId: p.podId, podName: p.podName ?? p.podId }))}
        onJoin={async (podId, code) => {
          try {
            const id = await joinViaInvite(podId, code);
            toast.success("Joined community");
            setJoinOpen(false);
            navigateToCommunity(podId, id);
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
