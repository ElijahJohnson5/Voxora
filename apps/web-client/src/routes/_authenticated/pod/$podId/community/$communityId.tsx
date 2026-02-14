import { createFileRoute, Outlet } from "@tanstack/react-router";
import { useCommunityStore } from "@/stores/communities";

export const Route = createFileRoute(
  "/_authenticated/pod/$podId/community/$communityId",
)({
  loader: ({ params }) => {
    useCommunityStore
      .getState()
      .setActive(params.podId, params.communityId);
  },
  component: CommunityLayout,
});

function CommunityLayout() {
  return <Outlet />;
}
