import { createFileRoute, Outlet } from "@tanstack/react-router";
import { usePodStore } from "@/stores/pod";
import { useCommunityStore } from "@/stores/communities";

export const Route = createFileRoute("/_authenticated/pod/$podId")({
  loader: ({ params }) => {
    usePodStore.getState().setActivePod(params.podId);
    useCommunityStore.getState().setActive(params.podId, "");
  },
  component: PodLayout,
});

function PodLayout() {
  return <Outlet />;
}
