import { useState } from "react";
import { useMatch, useNavigate } from "@tanstack/react-router";
import { Plus, ArrowDownToLine } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { useAuthStore } from "@/stores/auth";
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
  const {
    communities,
    channels,
    createCommunity,
    joinViaInvite,
  } = useCommunityStore();

  // Read active IDs from URL — always in sync, no flash
  const communityMatch = useMatch({
    from: "/_authenticated/community/$communityId",
    shouldThrow: false,
  });
  const channelMatch = useMatch({
    from: "/_authenticated/community/$communityId/channel/$channelId",
    shouldThrow: false,
  });
  const settingsMatch = useMatch({
    from: "/_authenticated/settings",
    shouldThrow: false,
  });
  const activeCommunityId = communityMatch?.params.communityId ?? null;
  const activeChannelId = channelMatch?.params.channelId ?? null;

  const [createOpen, setCreateOpen] = useState(false);
  const [joinOpen, setJoinOpen] = useState(false);

  const communityList = Object.values(communities);
  const activeCommunity = activeCommunityId
    ? communities[activeCommunityId]
    : null;
  const activeChannels = activeCommunityId
    ? (channels[activeCommunityId] ?? [])
    : [];

  function navigateToCommunity(communityId: string) {
    const communityChannels = channels[communityId] ?? [];
    const community = communities[communityId];
    const defaultChannel =
      community?.default_channel ?? communityChannels[0]?.id;
    if (defaultChannel) {
      navigate({
        to: "/community/$communityId/channel/$channelId",
        params: { communityId, channelId: defaultChannel },
      });
    }
  }

  return (
    <div className={cn("flex h-full shrink-0 border-r border-border", settingsMatch ? "w-16" : "w-60")}>
      {/* Community icon strip */}
      <TooltipProvider delayDuration={100}>
        <div className="flex w-16 flex-col items-center gap-2 border-r border-border bg-secondary/50">
          <ScrollArea className="flex-1 w-full">
            <div className="flex flex-col items-center gap-2 py-3">
              {communityList.map((community) => (
                <Tooltip key={community.id}>
                  <TooltipTrigger asChild>
                    <button
                      onClick={() => navigateToCommunity(community.id)}
                      className="flex items-center justify-center"
                    >
                      <Avatar
                        className={cn(
                          "h-10 w-10 cursor-pointer transition-all",
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
                  <TooltipContent side="right">{community.name}</TooltipContent>
                </Tooltip>
              ))}
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
                <Avatar className={cn("h-10 w-10 cursor-pointer transition-all hover:ring-2 hover:ring-primary", settingsMatch && "ring-2 ring-primary")}>
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

      {/* Channel list — hidden on settings page */}
      {!settingsMatch && (
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
                    if (!activeCommunityId) return;
                    navigate({
                      to: "/community/$communityId/channel/$channelId",
                      params: {
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
        onCreate={async (name, description) => {
          try {
            const id = await createCommunity(name, description);
            toast.success("Community created");
            setCreateOpen(false);
            navigateToCommunity(id);
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
        onJoin={async (code) => {
          try {
            const id = await joinViaInvite(code);
            toast.success("Joined community");
            setJoinOpen(false);
            navigateToCommunity(id);
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
