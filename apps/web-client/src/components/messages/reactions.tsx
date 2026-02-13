import { useMessageStore, type Reaction } from "@/stores/messages";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

interface MessageReactionsProps {
  reactions: Reaction[];
  channelId: string;
  messageId: string;
}

export function MessageReactions({
  reactions,
  channelId,
  messageId,
}: MessageReactionsProps) {
  const addReaction = useMessageStore((s) => s.addReaction);
  const removeReaction = useMessageStore((s) => s.removeReaction);

  const handleToggle = (emoji: string, alreadyReacted: boolean) => {
    if (alreadyReacted) {
      removeReaction(channelId, messageId, emoji);
    } else {
      addReaction(channelId, messageId, emoji);
    }
  };

  return (
    <div className="mt-1 flex flex-wrap gap-1">
      {reactions.map((r) => (
        <Badge
          key={r.emoji}
          variant="outline"
          className={cn(
            "cursor-pointer select-none gap-1 px-1.5 py-0.5 text-xs",
            r.me && "border-primary/50 bg-primary/10",
          )}
          onClick={() => handleToggle(r.emoji, r.me)}
        >
          <span>{r.emoji}</span>
          <span className="font-mono">{r.count}</span>
        </Badge>
      ))}
    </div>
  );
}
