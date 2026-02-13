import { useEffect } from "react";
import { createFileRoute, Outlet } from "@tanstack/react-router";
import { useCommunityStore } from "@/stores/communities";

export const Route = createFileRoute("/_authenticated/community/$communityId")({
  component: CommunityLayout,
});

function CommunityLayout() {
  const { communityId } = Route.useParams();
  const setActiveCommunity = useCommunityStore((s) => s.setActiveCommunity);

  useEffect(() => {
    setActiveCommunity(communityId);
  }, [communityId, setActiveCommunity]);

  return <Outlet />;
}
