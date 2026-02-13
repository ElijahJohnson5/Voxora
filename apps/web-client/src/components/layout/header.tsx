import { Moon, Sun, Monitor } from "lucide-react";
import { useMatch } from "@tanstack/react-router";
import { useTheme } from "@/lib/theme";
import { useCommunityStore } from "@/stores/communities";
import { useGatewayStatus } from "@/lib/gateway/useGatewayStatus";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

const themeOptions = [
  { value: "light" as const, icon: Sun, label: "Light" },
  { value: "dark" as const, icon: Moon, label: "Dark" },
  { value: "system" as const, icon: Monitor, label: "System" },
];

const statusConfig = {
  connected: { label: "Connected", dotClass: "bg-green-500" },
  connecting: { label: "Connecting…", dotClass: "bg-yellow-500 animate-pulse" },
  reconnecting: {
    label: "Reconnecting…",
    dotClass: "bg-yellow-500 animate-pulse",
  },
  disconnected: { label: "Disconnected", dotClass: "bg-red-500" },
} as const;

export function Header() {
  const { theme, setTheme } = useTheme();
  const channels = useCommunityStore((s) => s.channels);
  const gatewayStatus = useGatewayStatus();

  // Read IDs directly from the URL — always in sync, no flash
  const channelMatch = useMatch({
    from: "/_authenticated/community/$communityId/channel/$channelId",
    shouldThrow: false,
  });
  const communityId = channelMatch?.params.communityId;
  const channelId = channelMatch?.params.channelId;

  const channelList = communityId ? (channels[communityId] ?? []) : [];
  const activeChannel = channelList.find((c) => c.id === channelId);

  const current =
    themeOptions.find((o) => o.value === theme) ?? themeOptions[2];
  const Icon = current.icon;

  return (
    <div className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
      <div className="flex items-center gap-2">
        {activeChannel ? (
          <>
            <span className="text-muted-foreground">#</span>
            <h1 className="text-sm font-semibold">{activeChannel.name}</h1>
            {activeChannel.topic && (
              <>
                <Separator orientation="vertical" className="mx-2 h-4" />
                <span className="text-xs text-muted-foreground">
                  {activeChannel.topic}
                </span>
              </>
            )}
          </>
        ) : (
          <span className="text-sm text-muted-foreground">
            Select a channel
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        {gatewayStatus !== "connected" && (
          <Badge variant="outline" className="gap-1.5 text-xs font-normal">
            <span
              className={cn(
                "inline-block h-2 w-2 rounded-full",
                statusConfig[gatewayStatus].dotClass,
              )}
            />
            {statusConfig[gatewayStatus].label}
          </Badge>
        )}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground">
              <Icon className="size-4" />
              <span className="hidden sm:inline">{current.label}</span>
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuRadioGroup
              value={theme}
              onValueChange={(v) => setTheme(v as "light" | "dark" | "system")}
            >
              {themeOptions.map((option) => (
                <DropdownMenuRadioItem key={option.value} value={option.value}>
                  <option.icon className="size-4" />
                  {option.label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
