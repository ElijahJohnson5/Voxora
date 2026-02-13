import { createFileRoute, Outlet } from "@tanstack/react-router";

export const Route = createFileRoute("/_authenticated/community/$communityId")({
  component: CommunityLayout,
});

function CommunityLayout() {
  const { communityId } = Route.useParams();

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-border px-4 py-2">
        <h2 className="text-sm font-semibold text-muted-foreground">
          Community: {communityId}
        </h2>
      </div>
      <div className="flex-1">
        <Outlet />
      </div>
    </div>
  );
}
