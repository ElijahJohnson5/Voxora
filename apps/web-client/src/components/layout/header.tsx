import { Moon, Sun, Monitor, Pin } from "lucide-react";
import { useMatch } from "@tanstack/react-router";
import { useTheme } from "@/lib/theme";
import { useCommunityStore } from "@/stores/communities";
import { usePinStore } from "@/stores/pins";
import { useGatewayStatus } from "@/lib/gateway/useGatewayStatus";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Popover,
  PopoverTrigger,
  PopoverContent,
} from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { PinnedMessages } from "@/components/messages/pinned-messages";

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

  // Read IDs directly from the URL — always in sync, no flash
  const channelMatch = useMatch({
    from: "/_authenticated/pod/$podId/community/$communityId/channel/$channelId",
    shouldThrow: false,
  });
  const podId = channelMatch?.params.podId;
  const communityId = channelMatch?.params.communityId;
  const channelId = channelMatch?.params.channelId;

  const gatewayStatus = useGatewayStatus(podId);

  const channelList =
    podId && communityId ? (channels[podId]?.[communityId] ?? []) : [];
  const activeChannel = channelList.find((c) => c.id === channelId);

  const pinKey = podId && channelId ? `${podId}:${channelId}` : null;
  const pinCount = usePinStore((s) =>
    pinKey ? (s.byChannel[pinKey]?.length ?? 0) : 0,
  );

  const current =
    themeOptions.find((o) => o.value === theme) ?? themeOptions[2];
  const Icon = current.icon;

  return (
    <div className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
      <div className="flex items-center gap-2">
        {activeChannel && (
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
        )}
      </div>
      <div className="flex items-center gap-2">
        {podId && channelId && (
          <Popover>
            <PopoverTrigger asChild>
              <button className="relative flex items-center rounded-md px-2 py-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground">
                <Pin className="size-4" />
                {pinCount > 0 && (
                  <span className="absolute -top-1 -right-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] font-medium text-primary-foreground">
                    {pinCount}
                  </span>
                )}
              </button>
            </PopoverTrigger>
            <PopoverContent align="end" className="w-auto p-0">
              <PinnedMessages podId={podId} channelId={channelId} />
            </PopoverContent>
          </Popover>
        )}
        {podId && gatewayStatus !== "connected" && (
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
