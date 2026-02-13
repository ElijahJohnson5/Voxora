import { cn } from "@/lib/utils";

const communities = [
  { id: "1", name: "Voxora Dev", initials: "VD" },
  { id: "2", name: "Gaming Hub", initials: "GH" },
  { id: "3", name: "Music Fans", initials: "MF" },
];

const channels = [
  { id: "general", name: "general" },
  { id: "random", name: "random" },
  { id: "dev", name: "dev" },
  { id: "music", name: "music" },
];

export function Sidebar() {
  return (
    <div className="flex h-full w-60 shrink-0 border-r border-border">
      {/* Community icon strip */}
      <div className="flex w-16 flex-col items-center gap-2 border-r border-border bg-secondary/50 py-3">
        {communities.map((community) => (
          <button
            key={community.id}
            title={community.name}
            className={cn(
              "flex h-10 w-10 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground transition-colors hover:bg-primary hover:text-primary-foreground",
            )}
          >
            {community.initials}
          </button>
        ))}
        <div className="my-1 h-px w-8 bg-border" />
        <button
          title="Add Community"
          className="flex h-10 w-10 items-center justify-center rounded-full bg-muted text-lg text-muted-foreground transition-colors hover:bg-primary hover:text-primary-foreground"
        >
          +
        </button>
      </div>

      {/* Channel list */}
      <div className="flex flex-1 flex-col overflow-y-auto">
        <div className="flex h-12 items-center border-b border-border px-3">
          <h2 className="text-sm font-semibold">Voxora Dev</h2>
        </div>
        <nav className="flex-1 space-y-0.5 px-2 py-2">
          {channels.map((channel) => (
            <button
              key={channel.id}
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
            >
              <span className="text-xs">#</span>
              {channel.name}
            </button>
          ))}
        </nav>
      </div>
    </div>
  );
}
