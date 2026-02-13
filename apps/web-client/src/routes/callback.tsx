import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { handleCallback } from "@/lib/oidc";
import { useAuthStore } from "@/stores/auth";
import { hubApi } from "@/lib/api/hub-client";

export const Route = createFileRoute("/callback")({
  component: CallbackPage,
});

function CallbackPage() {
  const navigate = useNavigate();
  const setTokens = useAuthStore((s) => s.setTokens);
  const setUser = useAuthStore((s) => s.setUser);
  const [error, setError] = useState<string | null>(null);
  const exchanged = useRef(false);

  useEffect(() => {
    if (exchanged.current) return;
    exchanged.current = true;

    const params = new URLSearchParams(window.location.search);
    const code = params.get("code");
    const state = params.get("state");

    if (!code || !state) {
      setError("Missing authorization code or state");
      return;
    }

    const codeNonNull = code;
    const stateNonNull = state;

    async function exchange() {
      try {
        const tokens = await handleCallback(codeNonNull, stateNonNull);
        setTokens(tokens);

        // Fetch user profile
        const { data } = await hubApi.GET("/api/v1/users/@me");
        if (data) {
          setUser({
            id: data.id,
            username: data.username,
            displayName: data.display_name,
            email: data.email ?? null,
            avatarUrl: data.avatar_url ?? null,
          });
        }

        navigate({ to: "/" });
      } catch (err) {
        setError(err instanceof Error ? err.message : "Login failed");
      }
    }

    exchange();
  }, [navigate, setTokens, setUser]);

  if (error) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="flex flex-col items-center gap-4 text-center">
          <p className="text-sm text-destructive">{error}</p>
          <a href="/login" className="text-sm text-primary underline">
            Try again
          </a>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="flex flex-col items-center gap-4">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
        <p className="text-sm text-muted-foreground">Processing login...</p>
      </div>
    </div>
  );
}
