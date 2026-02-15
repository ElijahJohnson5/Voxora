import { useEffect } from "react";
import { createFileRoute, Outlet, redirect, useMatch } from "@tanstack/react-router";
import { Sidebar } from "@/components/layout/sidebar";
import { MemberList } from "@/components/layout/member-list";
import { Header } from "@/components/layout/header";
import { useAuthStore } from "@/stores/auth";
import { initPresenceIdle } from "@/lib/gateway/presence-idle";

export const Route = createFileRoute("/_authenticated")({
  beforeLoad: () => {
    if (!useAuthStore.getState().isAuthenticated()) {
      throw redirect({ to: "/login" });
    }
  },
  component: AuthenticatedLayout,
});

function AuthenticatedLayout() {
  useEffect(() => {
    initPresenceIdle();
  }, []);

  const channelMatch = useMatch({
    from: "/_authenticated/pod/$podId/community/$communityId/channel/$channelId",
    shouldThrow: false,
  });
  const settingsMatch = useMatch({
    from: "/_authenticated/settings",
    shouldThrow: false,
  });

  return (
    <div className="flex h-full">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        {!settingsMatch && <Header />}
        <main className="flex min-h-0 flex-1">
          <div className="flex-1 overflow-y-auto">
            <Outlet />
          </div>
          {channelMatch && <MemberList />}
        </main>
      </div>
    </div>
  );
}
