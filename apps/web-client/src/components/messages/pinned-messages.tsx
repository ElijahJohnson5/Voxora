import { useEffect } from "react";
import { usePinStore } from "@/stores/pins";
import { useCommunityStore } from "@/stores/communities";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { Pin } from "lucide-react";
import { RichTextContent } from "./rich-text-content";

function channelKey(podId: string, channelId: string): string {
  return `${podId}:${channelId}`;
}

function getInitials(name: string): string {
  const parts = name.trim().split(/\s+/);
  if (parts.length >= 2) {
    return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
  }
  return name.slice(0, 2).toUpperCase();
}

function formatTimestamp(dateStr: string): string {
  const date = new Date(dateStr);
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

interface PinnedMessagesProps {
  podId: string;
  channelId: string;
}

export function PinnedMessages({ podId, channelId }: PinnedMessagesProps) {
  const key = channelKey(podId, channelId);
  const pins = usePinStore((s) => s.byChannel[key]);
  const loading = usePinStore((s) => s.loading[key] ?? false);
  const fetchPins = usePinStore((s) => s.fetchPins);
  const unpinMessage = usePinStore((s) => s.unpinMessage);

  const activePodId = useCommunityStore((s) => s.activePodId);
  const activeCommunityId = useCommunityStore((s) => s.activeCommunityId);
  const members = useCommunityStore((s) => {
    const pid = activePodId ?? podId;
    if (!pid || !activeCommunityId) return [];
    return s.members[pid]?.[activeCommunityId] ?? [];
  });

  useEffect(() => {
    if (!pins) {
      fetchPins(podId, channelId);
    }
  }, [pins, fetchPins, podId, channelId]);

  if (loading) {
    return (
      <div className="w-80 space-y-3 p-3">
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="flex items-start gap-2">
            <Skeleton className="size-8 rounded-full" />
            <div className="space-y-1.5">
              <Skeleton className="h-3 w-24" />
              <Skeleton className="h-3 w-48" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  if (!pins || pins.length === 0) {
    return (
      <div className="flex w-80 flex-col items-center justify-center gap-2 p-6 text-center">
        <Pin className="size-8 text-muted-foreground" />
        <p className="text-sm font-medium">No pinned messages</p>
        <p className="text-xs text-muted-foreground">
          Pin important messages so they're easy to find later.
        </p>
      </div>
    );
  }

  return (
    <ScrollArea className="max-h-96 w-80">
      <div className="space-y-1 p-2">
        {pins.map((msg) => {
          const member = members.find((m) => m.user_id === msg.author_id);
          const authorDisplay =
            member?.nickname ??
            member?.display_name ??
            member?.username ??
            msg.author_id.slice(0, 8);
          const avatarUrl = member?.avatar_url;

          return (
            <div
              key={msg.id}
              className="rounded-md border border-border bg-card p-3"
            >
              <div className="flex items-center gap-2">
                <Avatar className="size-6">
                  {avatarUrl && (
                    <AvatarImage src={avatarUrl} alt={authorDisplay} />
                  )}
                  <AvatarFallback className="text-[10px]">
                    {getInitials(authorDisplay)}
                  </AvatarFallback>
                </Avatar>
                <span className="text-xs font-semibold">{authorDisplay}</span>
                <span className="text-[10px] text-muted-foreground">
                  {formatTimestamp(msg.created_at)}
                </span>
              </div>
              <div className="mt-1.5 text-sm">
                <RichTextContent content={msg.content} />
              </div>
              <div className="mt-2 flex justify-end">
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 px-2 text-xs text-muted-foreground"
                  onClick={() => unpinMessage(podId, channelId, msg.id)}
                >
                  Unpin
                </Button>
              </div>
            </div>
          );
        })}
      </div>
    </ScrollArea>
  );
}
