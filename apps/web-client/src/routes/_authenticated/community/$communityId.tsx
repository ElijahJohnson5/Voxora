import { createFileRoute, Outlet } from "@tanstack/react-router";
import { useCommunityStore } from "@/stores/communities";

export const Route = createFileRoute("/_authenticated/community/$communityId")({
  loader: ({ params }) => {
    useCommunityStore.getState().setActiveCommunity(params.communityId);
  },
  component: CommunityLayout,
});

function CommunityLayout() {
  return <Outlet />;
}
