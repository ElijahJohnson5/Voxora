import { useEffect, useState, useCallback } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { toast } from "sonner";
import { Search, Wifi, WifiOff, Users, Server, Loader2 } from "lucide-react";
import { hubApi } from "@/lib/api/hub-client";
import { usePodStore, type PodConnectionData } from "@/stores/pod";
import { useCommunityStore } from "@/stores/communities";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import type { components } from "@/lib/api/hub";

type PodResponse = components["schemas"]["PodResponse"];

export const Route = createFileRoute("/_authenticated/")({
  component: HomePage,
});

function HomePage() {
  const navigate = useNavigate();
  const pods = usePodStore((s) => s.pods);
  const connectToPod = usePodStore((s) => s.connectToPod);
  const disconnectFromPod = usePodStore((s) => s.disconnectFromPod);
  const communities = useCommunityStore((s) => s.communities);
  const channels = useCommunityStore((s) => s.channels);

  const [myPods, setMyPods] = useState<PodResponse[]>([]);
  const [discoverPods, setDiscoverPods] = useState<PodResponse[]>([]);
  const [loadingMyPods, setLoadingMyPods] = useState(true);
  const [loadingDiscover, setLoadingDiscover] = useState(true);
  const [search, setSearch] = useState("");
  const [connectingPodId, setConnectingPodId] = useState<string | null>(null);

  // Fetch my pods
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const { data, error } = await hubApi.GET("/api/v1/users/@me/pods");
        if (!cancelled && data && !error) {
          setMyPods(data.data);
        }
      } catch {
        // silently fail
      } finally {
        if (!cancelled) setLoadingMyPods(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Fetch discoverable pods
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const { data, error } = await hubApi.GET("/api/v1/pods", {
          params: { query: { sort: "popular", limit: 25 } },
        });
        if (!cancelled && data && !error) {
          setDiscoverPods(data.data);
        }
      } catch {
        // silently fail
      } finally {
        if (!cancelled) setLoadingDiscover(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const handleConnect = useCallback(
    async (pod: PodResponse) => {
      setConnectingPodId(pod.id);
      try {
        await connectToPod(pod.id, pod.url, pod.name, pod.icon_url ?? undefined);
        toast.success(`Connected to ${pod.name}`);
      } catch (err) {
        toast.error(
          err instanceof Error ? err.message : "Failed to connect to pod",
        );
      } finally {
        setConnectingPodId(null);
      }
    },
    [connectToPod],
  );

  const handleDisconnect = useCallback(
    (podId: string, podName: string) => {
      disconnectFromPod(podId);
      toast.success(`Disconnected from ${podName}`);
    },
    [disconnectFromPod],
  );

  const handleNavigateToPod = useCallback(
    (podId: string) => {
      const podCommunities = Object.values(communities[podId] ?? {});
      if (podCommunities.length === 0) return;
      const first = podCommunities[0];
      const podChannels = channels[podId]?.[first.id] ?? [];
      const channelId = first.default_channel ?? podChannels[0]?.id;
      if (channelId) {
        navigate({
          to: "/pod/$podId/community/$communityId/channel/$channelId",
          params: { podId, communityId: first.id, channelId },
        });
      }
    },
    [communities, channels, navigate],
  );

  const connectedPodIds = new Set(
    Object.values(pods)
      .filter((p) => p.connected || p.connecting)
      .map((p) => p.podId),
  );

  // Filter discover pods by search, exclude already-connected
  const filteredDiscover = discoverPods.filter((pod) => {
    if (connectedPodIds.has(pod.id)) return false;
    if (!search.trim()) return true;
    const q = search.toLowerCase();
    return (
      pod.name.toLowerCase().includes(q) ||
      (pod.description?.toLowerCase().includes(q) ?? false)
    );
  });

  return (
    <ScrollArea className="flex-1">
      <div className="mx-auto max-w-3xl space-y-8 p-6">
        <div>
          <h1 className="text-2xl font-bold">Pod Browser</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Connect to pods to join communities and start chatting.
          </p>
        </div>

        {/* My Pods */}
        <section>
          <h2 className="mb-3 text-sm font-semibold uppercase text-muted-foreground">
            My Pods
          </h2>
          {loadingMyPods ? (
            <div className="space-y-2">
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
            </div>
          ) : myPods.length === 0 && Object.keys(pods).length === 0 ? (
            <div className="rounded-lg border border-dashed border-border p-6 text-center">
              <Server className="mx-auto h-8 w-8 text-muted-foreground" />
              <p className="mt-2 text-sm text-muted-foreground">
                No pods yet. Discover and connect to pods below.
              </p>
            </div>
          ) : (
            <div className="space-y-2">
              {/* Show connected pods from store (includes ones not in myPods) */}
              {mergedMyPods(myPods, pods).map((pod) => {
                const conn = pods[pod.id];
                const isConnected = conn?.connected === true;
                const isConnecting = conn?.connecting === true;
                const hasCommunities =
                  Object.keys(communities[pod.id] ?? {}).length > 0;

                return (
                  <div
                    key={pod.id}
                    className="flex items-center gap-3 rounded-lg border border-border p-3 transition-colors hover:bg-accent/50"
                  >
                    <Avatar className="h-10 w-10 shrink-0">
                      {pod.icon_url && (
                        <AvatarImage src={pod.icon_url} alt={pod.name} />
                      )}
                      <AvatarFallback className="text-xs font-medium">
                        {pod.name.slice(0, 2).toUpperCase()}
                      </AvatarFallback>
                    </Avatar>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-semibold">
                          {pod.name}
                        </span>
                        {isConnected ? (
                          <Wifi className="h-3.5 w-3.5 shrink-0 text-green-500" />
                        ) : isConnecting ? (
                          <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-muted-foreground" />
                        ) : (
                          <WifiOff className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                        )}
                      </div>
                      {pod.description && (
                        <p className="truncate text-xs text-muted-foreground">
                          {pod.description}
                        </p>
                      )}
                      <div className="mt-0.5 flex items-center gap-2 text-xs text-muted-foreground">
                        <span className="flex items-center gap-0.5">
                          <Users className="h-3 w-3" />
                          {pod.member_count}
                        </span>
                        {pod.online_count > 0 && (
                          <span>{pod.online_count} online</span>
                        )}
                        {pod.region && <Badge variant="outline" className="h-4 px-1 text-[10px]">{pod.region}</Badge>}
                      </div>
                    </div>
                    <div className="flex shrink-0 gap-1.5">
                      {isConnected && hasCommunities && (
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => handleNavigateToPod(pod.id)}
                        >
                          Open
                        </Button>
                      )}
                      {isConnected ? (
                        <Button
                          size="sm"
                          variant="ghost"
                          className="text-destructive hover:text-destructive"
                          onClick={() => handleDisconnect(pod.id, pod.name)}
                        >
                          Disconnect
                        </Button>
                      ) : isConnecting ? (
                        <Button size="sm" variant="outline" disabled>
                          <Loader2 className="mr-1 h-3 w-3 animate-spin" />
                          Connecting
                        </Button>
                      ) : (
                        <Button
                          size="sm"
                          onClick={() =>
                            handleConnect({
                              ...pod,
                              url: conn?.podUrl ?? pod.url,
                            })
                          }
                          disabled={connectingPodId === pod.id}
                        >
                          {connectingPodId === pod.id ? (
                            <>
                              <Loader2 className="mr-1 h-3 w-3 animate-spin" />
                              Connecting
                            </>
                          ) : (
                            "Connect"
                          )}
                        </Button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>

        {/* Discover Pods */}
        <section>
          <div className="mb-3 flex items-center justify-between">
            <h2 className="text-sm font-semibold uppercase text-muted-foreground">
              Discover Pods
            </h2>
          </div>
          <div className="relative mb-3">
            <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              placeholder="Search pods..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="pl-8"
            />
          </div>
          {loadingDiscover ? (
            <div className="grid gap-3 sm:grid-cols-2">
              <Skeleton className="h-32" />
              <Skeleton className="h-32" />
              <Skeleton className="h-32" />
              <Skeleton className="h-32" />
            </div>
          ) : filteredDiscover.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border p-6 text-center">
              <p className="text-sm text-muted-foreground">
                {search.trim()
                  ? "No pods match your search."
                  : "No pods available to discover."}
              </p>
            </div>
          ) : (
            <div className="grid gap-3 sm:grid-cols-2">
              {filteredDiscover.map((pod) => (
                <div
                  key={pod.id}
                  className="flex flex-col rounded-lg border border-border p-4 transition-colors hover:bg-accent/50"
                >
                  <div className="flex items-start gap-3">
                    <Avatar className="h-10 w-10 shrink-0">
                      {pod.icon_url && (
                        <AvatarImage src={pod.icon_url} alt={pod.name} />
                      )}
                      <AvatarFallback className="text-xs font-medium">
                        {pod.name.slice(0, 2).toUpperCase()}
                      </AvatarFallback>
                    </Avatar>
                    <div className="min-w-0 flex-1">
                      <span className="truncate text-sm font-semibold">
                        {pod.name}
                      </span>
                      {pod.description && (
                        <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">
                          {pod.description}
                        </p>
                      )}
                    </div>
                  </div>
                  <div className="mt-3 flex items-center justify-between">
                    <div className="flex items-center gap-3 text-xs text-muted-foreground">
                      <span className="flex items-center gap-0.5">
                        <Users className="h-3 w-3" />
                        {pod.member_count}
                      </span>
                      {pod.online_count > 0 && (
                        <span className="text-green-500">
                          {pod.online_count} online
                        </span>
                      )}
                      {pod.community_count > 0 && (
                        <span>
                          {pod.community_count}{" "}
                          {pod.community_count === 1
                            ? "community"
                            : "communities"}
                        </span>
                      )}
                    </div>
                    <Button
                      size="sm"
                      onClick={() => handleConnect(pod)}
                      disabled={connectingPodId === pod.id}
                    >
                      {connectingPodId === pod.id ? (
                        <>
                          <Loader2 className="mr-1 h-3 w-3 animate-spin" />
                          Joining
                        </>
                      ) : (
                        "Join"
                      )}
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
    </ScrollArea>
  );
}

/** Merge Hub's "my pods" with locally connected pods to show a unified list. */
function mergedMyPods(
  myPods: PodResponse[],
  localPods: Record<string, PodConnectionData>,
): PodResponse[] {
  const seen = new Set<string>();
  const result: PodResponse[] = [];

  // First add all locally connected/connecting pods
  for (const conn of Object.values(localPods)) {
    seen.add(conn.podId);
    // Try to find richer data from myPods
    const hubPod = myPods.find((p) => p.id === conn.podId);
    result.push(
      hubPod ?? {
        id: conn.podId,
        name: conn.podName,
        url: conn.podUrl,
        icon_url: conn.podIcon ?? null,
        description: null,
        status: "active",
        public: true,
        region: null,
        version: null,
        capabilities: [],
        owner_id: "",
        member_count: 0,
        online_count: 0,
        community_count: 0,
        max_members: 0,
        last_heartbeat: null,
        created_at: "",
        updated_at: "",
      },
    );
  }

  // Add any from myPods that aren't already shown
  for (const pod of myPods) {
    if (!seen.has(pod.id)) {
      result.push(pod);
    }
  }

  return result;
}
