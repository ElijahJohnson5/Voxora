import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { startLogin } from "@/lib/oidc";
import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/login")({
  component: LoginPage,
});

function LoginPage() {
  const navigate = useNavigate();

  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="w-full max-w-sm space-y-6 rounded-lg border border-border bg-card p-8 shadow-lg">
        <div className="space-y-2 text-center">
          <h1 className="text-2xl font-bold tracking-tight">
            Sign in to Voxora
          </h1>
          <p className="text-sm text-muted-foreground">
            Connect with your community
          </p>
        </div>
        <Button className="w-full" onClick={() => startLogin()}>
          Sign in
        </Button>
        <p className="text-center text-sm text-muted-foreground">
          Don&apos;t have an account?{" "}
          <button
            type="button"
            onClick={() => navigate({ to: "/signup" })}
            className="text-primary underline underline-offset-4 hover:text-primary/80"
          >
            Create one
          </button>
        </p>
      </div>
    </div>
  );
}
