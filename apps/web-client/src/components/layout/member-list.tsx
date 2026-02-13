import { useState, useEffect, useMemo } from "react";
import { X, Users } from "lucide-react";
import { useCommunityStore, type CommunityMember, type Role } from "@/stores/communities";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { ScrollArea } from "@/components/ui/scroll-area";

interface MemberGroup {
  label: string;
  members: CommunityMember[];
}

function groupMembersByRole(
  members: CommunityMember[],
  roles: Role[],
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
      result.push({ label: `${role.name} — ${group.length}`, members: group });
    }
  }

  if (ungrouped.length > 0) {
    result.push({ label: `Members — ${ungrouped.length}`, members: ungrouped });
  }

  return result;
}

export function MemberList() {
  const [collapsed, setCollapsed] = useState(false);
  const { activeCommunityId, members, roles, fetchMembers } =
    useCommunityStore();

  useEffect(() => {
    if (activeCommunityId) {
      fetchMembers(activeCommunityId);
    }
  }, [activeCommunityId, fetchMembers]);

  const memberList = useMemo(
    () => (activeCommunityId ? (members[activeCommunityId] ?? []) : []),
    [activeCommunityId, members],
  );
  const roleList = useMemo(
    () => (activeCommunityId ? (roles[activeCommunityId] ?? []) : []),
    [activeCommunityId, roles],
  );

  const memberGroups = useMemo(
    () => groupMembersByRole(memberList, roleList),
    [memberList, roleList],
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
              {group.members.map((member) => (
                <div
                  key={member.user_id}
                  className="flex items-center gap-2 rounded-md px-2 py-1 hover:bg-accent"
                >
                  <Avatar className="h-7 w-7">
                    <AvatarFallback className="text-xs font-medium">
                      {(member.nickname ?? member.user_id)[0]?.toUpperCase() ?? "?"}
                    </AvatarFallback>
                  </Avatar>
                  <span className="truncate text-sm">
                    {member.nickname ?? member.user_id}
                  </span>
                </div>
              ))}
            </div>
          ))}
        </div>
      </ScrollArea>
    </div>
  );
}
