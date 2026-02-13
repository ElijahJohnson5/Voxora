import { Suspense } from "react";
import { Await, createFileRoute } from "@tanstack/react-router";
import { useCommunityStore } from "@/stores/communities";
import { useMessageStore } from "@/stores/messages";
import { MessageList } from "@/components/messages/message-list";
import { MessageInput } from "@/components/messages/message-input";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute(
  "/_authenticated/community/$communityId/channel/$channelId",
)({
  loader: async ({ params }) => {
    const { communityId, channelId } = params;

    await useCommunityStore.getState().fetchCommunity(communityId);

    // Sync: set active channel (already available from READY)
    useCommunityStore.getState().setActiveChannel(channelId);

    // Sync: resolve channel info from store
    const channels = useCommunityStore.getState().channels[communityId] ?? [];
    const channel = channels.find((c) => c.id === channelId);

    if (!channel) {
      throw new Response("Channel not found", { status: 404 });
    }

    // Only defer if messages aren't already cached
    const existing = useMessageStore.getState().byChannel[channelId];
    const hasCached = existing && existing.messages.length > 0;

    if (hasCached) {
      return { channel, deferredMessages: undefined };
    }

    const messagesPromise = useMessageStore.getState().fetchMessages(channelId);
    return { channel, deferredMessages: messagesPromise };
  },
  component: ChannelView,
});

function ChannelView() {
  const { channelId } = Route.useParams();
  const { channel, deferredMessages } = Route.useLoaderData();
  return (
    <div className="flex h-full flex-col">
      <TooltipProvider>
        {deferredMessages ? (
          <Suspense
            fallback={
              <MessageListSkeleton messageCount={channel.message_count} />
            }
          >
            <Await promise={deferredMessages}>
              {() => <MessageList channelId={channelId} />}
            </Await>
          </Suspense>
        ) : (
          <MessageList channelId={channelId} />
        )}
        <MessageInput
          channelId={channelId}
          placeholder={channel ? `Message #${channel.name}` : "Type a message…"}
        />
      </TooltipProvider>
    </div>
  );
}

function MessageListSkeleton({ messageCount }: { messageCount: number }) {
  if (messageCount === 0) {
    return (
      <div className="flex flex-1 flex-col overflow-y-auto">
        <div className="flex flex-1 items-end p-4">
          <div>
            <h3 className="text-lg font-semibold">Welcome to this channel!</h3>
            <p className="text-sm text-muted-foreground">
              This is the beginning of the conversation.
            </p>
          </div>
        </div>
        <div className="mt-auto flex flex-col px-4 py-2" />
      </div>
    );
  }

  return (
    <div className="flex flex-1 flex-col justify-end overflow-hidden p-4">
      <div className="space-y-4">
        {Array.from({ length: messageCount }).map((_, i) => (
          <div key={i} className="flex items-start gap-3">
            <Skeleton className="size-10 shrink-0 rounded-full" />
            <div className="space-y-2">
              <Skeleton className="h-4 w-32" />
              <Skeleton className="h-4 w-64" />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
