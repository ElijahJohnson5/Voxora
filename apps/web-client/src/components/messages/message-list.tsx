import {
  useEffect,
  useLayoutEffect,
  useRef,
  useCallback,
  useState,
  useMemo,
} from "react";
import { useMessageStore } from "@/stores/messages";
import { MessageItem } from "./message-item";
import { Skeleton } from "@/components/ui/skeleton";
import { usePodStore } from "@/stores/pod";

const EMPTY_MESSAGES: ReturnType<
  typeof useMessageStore.getState
>["byChannel"][string]["messages"] = [];

interface MessageListProps {
  channelId: string;
}

export function MessageList({ channelId }: MessageListProps) {
  const channelData = useMessageStore((s) => s.byChannel[channelId]);
  const messages = channelData?.messages ?? EMPTY_MESSAGES;
  const hasMore = channelData?.hasMore ?? true;
  const loading = channelData?.loading ?? false;
  const pending = useMessageStore((s) => s.pending);
  const pendingMessages = useMemo(
    () => Object.values(pending).filter((p) => p.channel_id === channelId),
    [pending, channelId],
  );
  const fetchMessages = useMessageStore((s) => s.fetchMessages);
  const currentUserId = usePodStore((s) => s.user?.id) ?? "";

  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [shouldAutoScroll, setShouldAutoScroll] = useState(true);
  const prevMessageCountRef = useRef(0);
  const hasInitialScrollRef = useRef(false);
  const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
  const handleEditStart = useCallback((id: string) => setEditingMessageId(id), []);
  const handleEditEnd = useCallback(() => setEditingMessageId(null), []);

  // Reset scroll state on channel change
  useEffect(() => {
    hasInitialScrollRef.current = false;
    prevMessageCountRef.current = 0;
  }, [channelId]);

  // Initial scroll — useLayoutEffect runs before paint so there's no visible flash
  useLayoutEffect(() => {
    if (hasInitialScrollRef.current) return;
    if (messages.length === 0) return;

    const el = scrollContainerRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
    hasInitialScrollRef.current = true;
    prevMessageCountRef.current = messages.length + pendingMessages.length;
  }, [messages.length, pendingMessages.length]);

  // Auto-scroll on new messages (only after initial scroll is done)
  useEffect(() => {
    if (!hasInitialScrollRef.current) return;
    const newCount = messages.length + pendingMessages.length;
    if (newCount > prevMessageCountRef.current && shouldAutoScroll) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
    prevMessageCountRef.current = newCount;
  }, [messages.length, pendingMessages.length, shouldAutoScroll]);

  // Track scroll position to determine auto-scroll behavior
  const handleScroll = useCallback(() => {
    const el = scrollContainerRef.current;
    if (!el) return;

    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setShouldAutoScroll(distanceFromBottom < 100);

    // Infinite scroll up — fetch more when near top
    if (el.scrollTop < 100 && hasMore && !loading) {
      const oldestMessage = messages[0];
      if (oldestMessage) {
        const prevHeight = el.scrollHeight;
        fetchMessages(channelId, oldestMessage.id).then(() => {
          // Preserve scroll position after prepending
          requestAnimationFrame(() => {
            const newHeight = el.scrollHeight;
            el.scrollTop = newHeight - prevHeight;
          });
        });
      }
    }
  }, [channelId, hasMore, loading, messages, fetchMessages]);

  // Group consecutive messages by the same author (compact mode)
  const isCompact = (index: number): boolean => {
    if (index === 0) return false;
    const prev = messages[index - 1];
    const curr = messages[index];
    if (!prev || !curr) return false;
    if (prev.author_id !== curr.author_id) return false;

    // Within 5 minutes
    const prevTime = new Date(prev.created_at).getTime();
    const currTime = new Date(curr.created_at).getTime();
    return currTime - prevTime < 5 * 60 * 1000;
  };

  return (
    <div
      ref={scrollContainerRef}
      className="styled-scrollbar flex flex-1 flex-col overflow-y-auto"
      onScroll={handleScroll}
    >
      {/* Loading skeleton for older message pagination */}
      {loading && messages.length > 0 && (
        <div className="space-y-4 p-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="flex items-start gap-3">
              <Skeleton className="size-10 shrink-0 rounded-full" />
              <div className="space-y-2">
                <Skeleton className="h-4 w-32" />
                <Skeleton className="h-4 w-64" />
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Empty state */}
      {!loading && messages.length === 0 && pendingMessages.length === 0 && (
        <div className="flex flex-1 items-end p-4">
          <div>
            <h3 className="text-lg font-semibold">Welcome to this channel!</h3>
            <p className="text-sm text-muted-foreground">
              This is the beginning of the conversation.
            </p>
          </div>
        </div>
      )}

      {/* Message list — mt-auto pushes messages to the bottom when content is short */}
      <div className="mt-auto flex flex-col px-4 py-2">
        {messages.map((msg, i) => (
          <MessageItem
            key={msg.id}
            message={msg}
            isOwn={msg.author_id === currentUserId}
            compact={isCompact(i)}
            editing={editingMessageId === msg.id}
            onEditStart={handleEditStart}
            onEditEnd={handleEditEnd}
          />
        ))}

        {/* Pending messages (optimistic) */}
        {pendingMessages.map((pm, i) => {
          // Determine compact by checking the previous message (real or pending)
          const prev =
            i === 0
              ? messages[messages.length - 1]
              : {
                  author_id: pendingMessages[i - 1].author_id,
                  created_at: pendingMessages[i - 1].created_at,
                };

          const isPendingCompact = (() => {
            if (!prev) return false;
            if (prev.author_id !== pm.author_id) return false;
            const prevTime = new Date(prev.created_at).getTime();
            const currTime = new Date(pm.created_at).getTime();
            return currTime - prevTime < 5 * 60 * 1000;
          })();

          return (
            <MessageItem
              key={`pending-${pm.nonce}`}
              message={{
                id: `pending-${pm.nonce}`,
                channel_id: pm.channel_id,
                author_id: pm.author_id,
                content: pm.content,
                type: 0,
                flags: 0,
                reply_to: pm.reply_to,
                edited_at: null,
                pinned: false,
                created_at: pm.created_at,
              }}
              isOwn={true}
              compact={isPendingCompact}
              pending
            />
          );
        })}
      </div>

      <div ref={bottomRef} />
    </div>
  );
}
