import { useEffect } from "react";
import { useTypingStore } from "@/stores/typing";
import { useChannel } from "./channel-context";

function channelKey(podId: string, channelId: string): string {
  return `${podId}:${channelId}`;
}

function formatTypingText(usernames: string[]): string {
  if (usernames.length === 0) return "";
  if (usernames.length === 1) return `${usernames[0]} is typing`;
  if (usernames.length === 2)
    return `${usernames[0]} and ${usernames[1]} are typing`;
  return `${usernames[0]}, ${usernames[1]}, and ${usernames.length - 2} other${usernames.length - 2 > 1 ? "s" : ""} are typing`;
}

export function TypingIndicator() {
  const { podId, channelId } = useChannel();
  const key = channelKey(podId, channelId);

  const typingUsers = useTypingStore((s) => s.byChannel[key] ?? []);
  const pruneExpired = useTypingStore((s) => s.pruneExpired);

  // Prune expired entries every second
  useEffect(() => {
    const timer = setInterval(pruneExpired, 1_000);
    return () => clearInterval(timer);
  }, [pruneExpired]);

  const usernames = typingUsers.map((u) => u.username);
  const text = formatTypingText(usernames);

  return (
    <div className="h-5 px-4 text-xs text-muted-foreground">
      {text && (
        <span>
          {text}
          <span className="typing-dots">
            <span className="animate-typing-dot-1">.</span>
            <span className="animate-typing-dot-2">.</span>
            <span className="animate-typing-dot-3">.</span>
          </span>
        </span>
      )}
    </div>
  );
}
