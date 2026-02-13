import { createFileRoute, Outlet, redirect } from "@tanstack/react-router";
import { Sidebar } from "@/components/layout/sidebar";
import { MemberList } from "@/components/layout/member-list";
import { Header } from "@/components/layout/header";

export const Route = createFileRoute("/_authenticated")({
  beforeLoad: () => {
    // TODO (C-2): Check auth state from Zustand store
    const isAuthenticated = true;
    if (!isAuthenticated) {
      throw redirect({ to: "/login" });
    }
  },
  component: AuthenticatedLayout,
});

function AuthenticatedLayout() {
  return (
    <div className="flex h-full">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <Header />
        <main className="flex min-h-0 flex-1">
          <div className="flex-1 overflow-y-auto">
            <Outlet />
          </div>
          <MemberList />
        </main>
      </div>
    </div>
  );
}
