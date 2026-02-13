import { useState } from "react";
import { cn } from "@/lib/utils";

const placeholderMembers = [
  { id: "1", name: "Alice", status: "online" as const },
  { id: "2", name: "Bob", status: "online" as const },
  { id: "3", name: "Charlie", status: "idle" as const },
  { id: "4", name: "Diana", status: "offline" as const },
];

export function MemberList() {
  const [collapsed, setCollapsed] = useState(false);

  if (collapsed) {
    return (
      <button
        onClick={() => setCollapsed(false)}
        className="flex h-full w-8 flex-shrink-0 items-start justify-center border-l border-border pt-3 text-muted-foreground hover:text-foreground"
        title="Show members"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
          <circle cx="9" cy="7" r="4" />
          <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
          <path d="M16 3.13a4 4 0 0 1 0 7.75" />
        </svg>
      </button>
    );
  }

  const online = placeholderMembers.filter((m) => m.status === "online");
  const idle = placeholderMembers.filter((m) => m.status === "idle");
  const offline = placeholderMembers.filter((m) => m.status === "offline");

  return (
    <div className="flex w-60 flex-shrink-0 flex-col border-l border-border">
      <div className="flex h-12 items-center justify-between border-b border-border px-3">
        <span className="text-xs font-semibold uppercase text-muted-foreground">
          Members
        </span>
        <button
          onClick={() => setCollapsed(true)}
          className="text-muted-foreground hover:text-foreground"
          title="Hide members"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
          </svg>
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-2 py-2">
        <MemberGroup label={`Online — ${online.length}`} members={online} />
        <MemberGroup label={`Idle — ${idle.length}`} members={idle} />
        <MemberGroup label={`Offline — ${offline.length}`} members={offline} />
      </div>
    </div>
  );
}

function MemberGroup({
  label,
  members,
}: {
  label: string;
  members: typeof placeholderMembers;
}) {
  if (members.length === 0) return null;

  return (
    <div className="mb-3">
      <h3 className="mb-1 px-2 text-xs font-semibold uppercase text-muted-foreground">
        {label}
      </h3>
      {members.map((member) => (
        <div
          key={member.id}
          className="flex items-center gap-2 rounded-md px-2 py-1 hover:bg-accent"
        >
          <div className="relative">
            <div className="flex h-7 w-7 items-center justify-center rounded-full bg-muted text-xs font-medium">
              {member.name[0]}
            </div>
            <span
              className={cn(
                "absolute -bottom-0.5 -right-0.5 h-3 w-3 rounded-full border-2 border-background",
                member.status === "online" && "bg-green-500",
                member.status === "idle" && "bg-yellow-500",
                member.status === "offline" && "bg-gray-500",
              )}
            />
          </div>
          <span className="text-sm">{member.name}</span>
        </div>
      ))}
    </div>
  );
}
