import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { hubApi } from "@/lib/api/hub-client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { startLogin } from "@/lib/oidc";

export const Route = createFileRoute("/signup")({
  component: SignupPage,
});

function SignupPage() {
  const navigate = useNavigate();
  const [error, setError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setError(null);
    setFieldErrors({});
    setLoading(true);

    const form = new FormData(e.currentTarget);
    const username = (form.get("username") as string).trim();
    const email = (form.get("email") as string).trim();
    const password = form.get("password") as string;
    const displayName = (form.get("display_name") as string).trim();

    const { error: apiError } = await hubApi.POST("/api/v1/users", {
      body: {
        username,
        email: email || null,
        password,
        display_name: displayName,
      },
    });

    if (apiError) {
      setLoading(false);
      const detail = (apiError as { error?: { message?: string; details?: { field: string; message: string }[] } }).error;
      if (detail?.details?.length) {
        const errs: Record<string, string> = {};
        for (const d of detail.details) errs[d.field] = d.message;
        setFieldErrors(errs);
      } else {
        setError(detail?.message || "Signup failed");
      }
      return;
    }

    // Account created — start OIDC login flow
    await startLogin();
  }

  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="w-full max-w-sm space-y-6 rounded-lg border border-border bg-card p-8 shadow-lg">
        <div className="space-y-2 text-center">
          <h1 className="text-2xl font-bold tracking-tight">
            Create an account
          </h1>
          <p className="text-sm text-muted-foreground">
            Join the Voxora community
          </p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1">
            <label htmlFor="username" className="text-sm font-medium">
              Username
            </label>
            <Input
              id="username"
              name="username"
              required
              autoComplete="username"
              placeholder="cooluser42"
            />
            {fieldErrors.username && (
              <p className="text-xs text-destructive">{fieldErrors.username}</p>
            )}
          </div>

          <div className="space-y-1">
            <label htmlFor="display_name" className="text-sm font-medium">
              Display name
            </label>
            <Input
              id="display_name"
              name="display_name"
              required
              placeholder="Cool User"
            />
            {fieldErrors.display_name && (
              <p className="text-xs text-destructive">
                {fieldErrors.display_name}
              </p>
            )}
          </div>

          <div className="space-y-1">
            <label htmlFor="email" className="text-sm font-medium">
              Email{" "}
              <span className="text-muted-foreground font-normal">
                (optional)
              </span>
            </label>
            <Input
              id="email"
              name="email"
              type="email"
              autoComplete="email"
              placeholder="you@example.com"
            />
            {fieldErrors.email && (
              <p className="text-xs text-destructive">{fieldErrors.email}</p>
            )}
          </div>

          <div className="space-y-1">
            <label htmlFor="password" className="text-sm font-medium">
              Password
            </label>
            <Input
              id="password"
              name="password"
              type="password"
              required
              autoComplete="new-password"
            />
            {fieldErrors.password && (
              <p className="text-xs text-destructive">{fieldErrors.password}</p>
            )}
          </div>

          {error && <p className="text-sm text-destructive">{error}</p>}

          <Button className="w-full" type="submit" disabled={loading}>
            {loading ? "Creating account..." : "Create account"}
          </Button>
        </form>

        <p className="text-center text-sm text-muted-foreground">
          Already have an account?{" "}
          <button
            type="button"
            onClick={() => navigate({ to: "/login" })}
            className="text-primary underline underline-offset-4 hover:text-primary/80"
          >
            Sign in
          </button>
        </p>
      </div>
    </div>
  );
}
