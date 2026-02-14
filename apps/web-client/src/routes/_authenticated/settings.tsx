import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { Moon, Sun, Monitor } from "lucide-react";
import { toast } from "sonner";
import { useAuthStore } from "@/stores/auth";
import { useTheme } from "@/lib/theme";
import { hubApi } from "@/lib/api/hub-client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Avatar, AvatarImage, AvatarFallback } from "@/components/ui/avatar";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";

export const Route = createFileRoute("/_authenticated/settings")({
  component: SettingsPage,
});

function getInitials(name: string) {
  return name
    .split(/\s+/)
    .map((w) => w[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
}

function SettingsPage() {
  const navigate = useNavigate();
  const user = useAuthStore((s) => s.user);
  const setUser = useAuthStore((s) => s.setUser);
  const clearTokens = useAuthStore((s) => s.clearTokens);

  const [displayName, setDisplayName] = useState(user?.displayName ?? "");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setError(null);
    setFieldErrors({});
    setLoading(true);

    const { data, error: apiError } = await hubApi.PATCH("/api/v1/users/@me", {
      body: { display_name: displayName.trim() },
    });

    if (apiError) {
      setLoading(false);
      const detail = (
        apiError as {
          error?: {
            message?: string;
            details?: { field: string; message: string }[];
          };
        }
      ).error;
      if (detail?.details?.length) {
        const errs: Record<string, string> = {};
        for (const d of detail.details) errs[d.field] = d.message;
        setFieldErrors(errs);
      } else {
        setError(detail?.message || "Update failed");
      }
      return;
    }

    if (data) {
      setUser({
        id: data.id,
        username: data.username,
        displayName: data.display_name,
        email: data.email ?? null,
        avatarUrl: data.avatar_url ?? null,
      });
      toast.success("Profile updated");
    }

    setLoading(false);
  }

  function handleLogout() {
    clearTokens();
    navigate({ to: "/login" });
  }

  if (!user) return null;

  return (
    <div className="flex min-h-full items-start justify-center p-6">
      <div className="w-full max-w-md space-y-6">
        <h1 className="text-2xl font-bold tracking-tight">Settings</h1>

        {/* Profile header */}
        <div className="flex items-center gap-4">
          <Avatar size="lg">
            {user.avatarUrl && (
              <AvatarImage src={user.avatarUrl} alt={user.displayName} />
            )}
            <AvatarFallback>{getInitials(user.displayName)}</AvatarFallback>
          </Avatar>
          <div className="min-w-0">
            <p className="truncate font-semibold">{user.displayName}</p>
            <p className="truncate text-sm text-muted-foreground">
              @{user.username}
            </p>
          </div>
        </div>

        <Separator />

        {/* Edit profile form */}
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1">
            <Label htmlFor="username">Username</Label>
            <Input id="username" value={user.username} disabled />
          </div>

          <div className="space-y-1">
            <Label htmlFor="email">Email</Label>
            <Input id="email" value={user.email ?? "Not set"} disabled />
          </div>

          <div className="space-y-1">
            <Label htmlFor="display_name">Display name</Label>
            <Input
              id="display_name"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              required
              placeholder="Your display name"
            />
            {fieldErrors.display_name && (
              <p className="text-xs text-destructive">
                {fieldErrors.display_name}
              </p>
            )}
          </div>

          {error && <p className="text-sm text-destructive">{error}</p>}

          <Button type="submit" disabled={loading}>
            {loading ? "Saving..." : "Save changes"}
          </Button>
        </form>

        <Separator />

        {/* Appearance */}
        <ThemeSwitcher />

        <Separator />

        {/* Logout */}
        <Button variant="destructive" onClick={handleLogout}>
          Log out
        </Button>
      </div>
    </div>
  );
}

const themeOptions = [
  { value: "light" as const, icon: Sun, label: "Light" },
  { value: "dark" as const, icon: Moon, label: "Dark" },
  { value: "system" as const, icon: Monitor, label: "System" },
];

function ThemeSwitcher() {
  const { theme, setTheme } = useTheme();

  return (
    <div className="space-y-2">
      <Label>Appearance</Label>
      <div className="flex gap-2">
        {themeOptions.map((option) => (
          <button
            key={option.value}
            type="button"
            onClick={() => setTheme(option.value)}
            className={cn(
              "flex flex-1 flex-col items-center gap-1.5 rounded-lg border border-border p-3 text-sm transition-colors hover:bg-accent",
              theme === option.value && "border-primary bg-primary/5",
            )}
          >
            <option.icon className="size-5" />
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}
