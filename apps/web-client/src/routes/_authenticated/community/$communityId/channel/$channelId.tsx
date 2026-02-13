import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute(
  "/_authenticated/community/$communityId/channel/$channelId",
)({
  component: ChannelView,
});

function ChannelView() {
  const { channelId } = Route.useParams();

  return (
    <div className="flex h-full flex-col">
      <div className="flex-1 overflow-y-auto p-4">
        <p className="text-muted-foreground">
          Messages for channel: {channelId}
        </p>
      </div>
      <div className="border-t border-border p-4">
        <div className="rounded-md border border-input bg-secondary px-4 py-2 text-sm text-muted-foreground">
          Message input placeholder
        </div>
      </div>
    </div>
  );
}
