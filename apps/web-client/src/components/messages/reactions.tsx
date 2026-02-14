import { useMessageStore, type Reaction } from "@/stores/messages";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { useChannel } from "./channel-context";

interface MessageReactionsProps {
  reactions: Reaction[];
  messageId: string;
}

export function MessageReactions({
  reactions,
  messageId,
}: MessageReactionsProps) {
  const { podId, channelId } = useChannel();
  const addReaction = useMessageStore((s) => s.addReaction);
  const removeReaction = useMessageStore((s) => s.removeReaction);

  const handleToggle = (emoji: string, alreadyReacted: boolean) => {
    if (alreadyReacted) {
      removeReaction(podId, channelId, messageId, emoji);
    } else {
      addReaction(podId, channelId, messageId, emoji);
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
