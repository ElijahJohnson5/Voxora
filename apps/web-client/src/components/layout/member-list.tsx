import { useState, useEffect, useMemo } from "react";
import { useMatch } from "@tanstack/react-router";
import { X, Users } from "lucide-react";
import { useCommunityStore, type CommunityMember, type Role } from "@/stores/communities";
import { usePresenceStore, type PresenceStatus } from "@/stores/presence";
import { Avatar, AvatarFallback, AvatarImage, AvatarBadge } from "@/components/ui/avatar";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

const STATUS_COLOR: Record<PresenceStatus, string> = {
  online: "bg-green-500",
  idle: "bg-yellow-500",
  dnd: "bg-red-500",
  offline: "bg-gray-500",
};

const STATUS_ORDER: Record<PresenceStatus, number> = {
  online: 0,
  idle: 1,
  dnd: 2,
  offline: 3,
};

interface MemberGroup {
  label: string;
  members: CommunityMember[];
}

function sortByPresence(
  members: CommunityMember[],
  presences: Record<string, PresenceStatus>,
): CommunityMember[] {
  return [...members].sort((a, b) => {
    const sa = STATUS_ORDER[presences[a.user_id] ?? "offline"];
    const sb = STATUS_ORDER[presences[b.user_id] ?? "offline"];
    return sa - sb;
  });
}

function groupMembersByRole(
  members: CommunityMember[],
  roles: Role[],
  presences: Record<string, PresenceStatus>,
): MemberGroup[] {
  const nonDefaultRoles = roles.filter((r) => !r.is_default);
  const roleMap = new Map(nonDefaultRoles.map((r) => [r.id, r]));

  const groups = new Map<string, CommunityMember[]>();
  const ungrouped: CommunityMember[] = [];

  for (const member of members) {
    // Find the member's highest role (lowest position number)
    let highestRole: Role | null = null;
    for (const roleId of member.roles) {
      const role = roleMap.get(roleId);
      if (role && (!highestRole || role.position < highestRole.position)) {
        highestRole = role;
      }
    }

    if (highestRole) {
      const group = groups.get(highestRole.id) ?? [];
      group.push(member);
      groups.set(highestRole.id, group);
    } else {
      ungrouped.push(member);
    }
  }

  const result: MemberGroup[] = [];

  // Add role groups sorted by position (highest rank = lowest position first)
  for (const role of nonDefaultRoles) {
    const group = groups.get(role.id);
    if (group && group.length > 0) {
      result.push({
        label: `${role.name} — ${group.length}`,
        members: sortByPresence(group, presences),
      });
    }
  }

  if (ungrouped.length > 0) {
    result.push({
      label: `Members — ${ungrouped.length}`,
      members: sortByPresence(ungrouped, presences),
    });
  }

  return result;
}

export function MemberList() {
  const [collapsed, setCollapsed] = useState(false);
  const { members, roles, fetchMembers } = useCommunityStore();
  const presenceByPod = usePresenceStore((s) => s.byPod);

  const channelMatch = useMatch({
    from: "/_authenticated/pod/$podId/community/$communityId/channel/$channelId",
    shouldThrow: false,
  });
  const podId = channelMatch?.params.podId ?? null;
  const communityId = channelMatch?.params.communityId ?? null;

  useEffect(() => {
    if (podId && communityId) {
      fetchMembers(podId, communityId);
    }
  }, [podId, communityId, fetchMembers]);

  const memberList = useMemo(
    () =>
      podId && communityId
        ? (members[podId]?.[communityId] ?? [])
        : [],
    [podId, communityId, members],
  );
  const roleList = useMemo(
    () =>
      podId && communityId
        ? (roles[podId]?.[communityId] ?? [])
        : [],
    [podId, communityId, roles],
  );

  const podPresences = useMemo(
    () => (podId ? (presenceByPod[podId] ?? {}) : {}),
    [podId, presenceByPod],
  );

  const memberGroups = useMemo(
    () => groupMembersByRole(memberList, roleList, podPresences),
    [memberList, roleList, podPresences],
  );

  if (collapsed) {
    return (
      <button
        onClick={() => setCollapsed(false)}
        className="flex h-full w-8 shrink-0 items-start justify-center border-l border-border pt-3 text-muted-foreground hover:text-foreground"
        title="Show members"
      >
        <Users className="h-4 w-4" />
      </button>
    );
  }

  return (
    <div className="flex w-60 shrink-0 flex-col border-l border-border">
      <div className="flex h-12 items-center justify-between border-b border-border px-3">
        <span className="text-xs font-semibold uppercase text-muted-foreground">
          Members
        </span>
        <button
          onClick={() => setCollapsed(true)}
          className="text-muted-foreground hover:text-foreground"
          title="Hide members"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
      <ScrollArea className="flex-1">
        <div className="px-2 py-2">
          {memberGroups.length === 0 && memberList.length === 0 && (
            <p className="px-2 text-xs text-muted-foreground">No members</p>
          )}
          {memberGroups.map((group) => (
            <div key={group.label} className="mb-3">
              <h3 className="mb-1 px-2 text-xs font-semibold uppercase text-muted-foreground">
                {group.label}
              </h3>
              {group.members.map((member) => {
                const displayName = member.display_name ?? member.username;
                const showNickname = member.nickname && member.nickname !== displayName;
                const status: PresenceStatus = podPresences[member.user_id] ?? "offline";
                return (
                  <div
                    key={member.user_id}
                    className={cn(
                      "flex items-center gap-2 rounded-md px-2 py-1 hover:bg-accent",
                      status === "offline" && "opacity-50",
                    )}
                  >
                    <Avatar className="h-7 w-7">
                      {member.avatar_url && <AvatarImage src={member.avatar_url} alt={displayName} />}
                      <AvatarFallback className="text-xs font-medium">
                        {displayName[0]?.toUpperCase() ?? "?"}
                      </AvatarFallback>
                      <AvatarBadge className={STATUS_COLOR[status]} />
                    </Avatar>
                    <span className="truncate text-sm">
                      {displayName}
                      {showNickname && (
                        <span className="ml-1 text-xs text-muted-foreground">({member.nickname})</span>
                      )}
                    </span>
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      </ScrollArea>
    </div>
  );
}
