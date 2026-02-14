import { useEffect, useRef, useCallback, useState, useMemo } from "react";
import { Virtualizer, type VirtualizerHandle } from "virtua";
import { ArrowDown } from "lucide-react";
import { useMessageStore, type Message } from "@/stores/messages";
import { MessageItem } from "./message-item";
import { usePodStore } from "@/stores/pod";
import { Button } from "@/components/ui/button";

const EMPTY_MESSAGES: Message[] = [];

interface MessageListProps {
  channelId: string;
}

interface ListItem {
  key: string;
  message: Message;
  isOwn: boolean;
  compact: boolean;
  pending: boolean;
}

function isCompactMessage(
  prev: { author_id: string; created_at: string } | undefined,
  curr: { author_id: string; created_at: string },
): boolean {
  if (!prev) return false;
  if (prev.author_id !== curr.author_id) return false;
  const prevTime = new Date(prev.created_at).getTime();
  const currTime = new Date(curr.created_at).getTime();
  return currTime - prevTime < 5 * 60 * 1000;
}

export function MessageList({ channelId }: MessageListProps) {
  const channelData = useMessageStore((s) => s.byChannel[channelId]);
  const messages = channelData?.messages ?? EMPTY_MESSAGES;
  const hasOlder = channelData?.hasOlder ?? true;
  const hasNewer = channelData?.hasNewer ?? false;
  const loading = channelData?.loading ?? false;
  const pending = useMessageStore((s) => s.pending);
  const pendingMessages = useMemo(
    () => Object.values(pending).filter((p) => p.channel_id === channelId),
    [pending, channelId],
  );
  const fetchMessages = useMessageStore((s) => s.fetchMessages);
  const currentUserId = usePodStore((s) => s.user?.id) ?? "";

  const ref = useRef<VirtualizerHandle>(null);
  const [shifting, setShifting] = useState(false);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
  const handleEditStart = useCallback(
    (id: string) => setEditingMessageId(id),
    [],
  );
  const handleEditEnd = useCallback(() => setEditingMessageId(null), []);

  const isAtBottomRef = useRef(true);
  const lastItemCountRef = useRef(0);
  const fetchingOlderRef = useRef(false);
  const fetchingNewerRef = useRef(false);
  const hasInitialScrollRef = useRef(false);
  const readyRef = useRef(false);

  // Build flat list of all items (messages + pending)
  const allItems = useMemo((): ListItem[] => {
    const result: ListItem[] = [];

    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];
      result.push({
        key: msg.id,
        message: msg,
        isOwn: msg.author_id === currentUserId,
        compact: isCompactMessage(messages[i - 1], msg),
        pending: false,
      });
    }

    for (let i = 0; i < pendingMessages.length; i++) {
      const pm = pendingMessages[i];
      const prev =
        i === 0 ? messages[messages.length - 1] : pendingMessages[i - 1];

      result.push({
        key: `pending-${pm.nonce}`,
        message: {
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
        },
        isOwn: true,
        compact: isCompactMessage(prev, pm),
        pending: true,
      });
    }

    return result;
  }, [messages, pendingMessages, currentUserId]);

  // Reset on channel change
  useEffect(() => {
    hasInitialScrollRef.current = false;
    lastItemCountRef.current = 0;
    isAtBottomRef.current = true;
    fetchingOlderRef.current = false;
    fetchingNewerRef.current = false;
    readyRef.current = false;
    setShifting(false);
    setIsAtBottom(true);
  }, [channelId]);

  // Initial scroll to bottom
  useEffect(() => {
    if (hasInitialScrollRef.current) return;
    if (allItems.length === 0) return;

    ref.current?.scrollToIndex(allItems.length - 1, { align: "end" });
    hasInitialScrollRef.current = true;
    lastItemCountRef.current = allItems.length;
    requestAnimationFrame(() => {
      readyRef.current = true;
    });
  }, [allItems.length]);

  // Handle item count changes (pagination and new messages)
  useEffect(() => {
    if (!hasInitialScrollRef.current) return;
    if (allItems.length <= lastItemCountRef.current) return;

    // Older messages prepended — shift handles scroll restoration
    if (fetchingOlderRef.current) {
      fetchingOlderRef.current = false;
      setShifting(false);
      lastItemCountRef.current = allItems.length;
      return;
    }

    // Newer messages appended via pagination
    if (fetchingNewerRef.current) {
      fetchingNewerRef.current = false;
      lastItemCountRef.current = allItems.length;
      return;
    }

    // New message from gateway/send — auto-scroll if at bottom or own send
    const lastItem = allItems[allItems.length - 1];
    lastItemCountRef.current = allItems.length;
    if (isAtBottomRef.current || lastItem?.pending) {
      ref.current?.scrollToIndex(allItems.length - 1, {
        align: "end",
        smooth: true,
      });
    }
  }, [allItems]);

  const scrollToBottom = useCallback(() => {
    ref.current?.scrollToIndex(allItems.length - 1, {
      align: "end",
      smooth: true,
    });
  }, [allItems.length]);

  return (
    <div className="relative flex flex-1 flex-col overflow-hidden">
      <div className="styled-scrollbar flex flex-1 flex-col overflow-y-auto">
        {allItems.length === 0 ? (
          <div className="flex flex-1 items-end p-4">
            <div>
              <h3 className="text-lg font-semibold">
                Welcome to this channel!
              </h3>
              <p className="text-sm text-muted-foreground">
                This is the beginning of the conversation.
              </p>
            </div>
          </div>
        ) : (
          <Virtualizer
            ref={ref}
            shift={shifting}
            bufferSize={550}
            onScroll={() => {
              if (!readyRef.current) return;
              if (!ref.current) return;

              const { scrollOffset, scrollSize, viewportSize } = ref.current;
              const distanceFromBottom =
                scrollSize - scrollOffset - viewportSize;
              const atBottom = distanceFromBottom < 100;
              isAtBottomRef.current = atBottom;
              setIsAtBottom(atBottom);

              // Fetch older messages when near top
              if (
                scrollOffset < 200 &&
                hasOlder &&
                !loading &&
                !fetchingOlderRef.current
              ) {
                const oldestMessage = messages[0];
                if (oldestMessage) {
                  setShifting(true);
                  fetchingOlderRef.current = true;
                  fetchMessages(channelId, { before: oldestMessage.id });
                }
              }

              // Fetch newer messages when near bottom
              if (
                distanceFromBottom < 200 &&
                hasNewer &&
                !loading &&
                !fetchingNewerRef.current
              ) {
                const newestMessage = messages[messages.length - 1];
                if (newestMessage) {
                  fetchingNewerRef.current = true;
                  fetchMessages(channelId, { after: newestMessage.id });
                }
              }
            }}
          >
            {allItems.map((item) => (
              <MessageItem
                key={item.key}
                message={item.message}
                isOwn={item.isOwn}
                compact={item.compact}
                pending={item.pending}
                editing={editingMessageId === item.message.id}
                onEditStart={handleEditStart}
                onEditEnd={handleEditEnd}
              />
            ))}
          </Virtualizer>
        )}
      </div>

      {!isAtBottom && allItems.length > 0 && (
        <div className="absolute inset-x-0 bottom-0 flex justify-center pb-2">
          <button
            type="button"
            onClick={scrollToBottom}
            className="flex items-center gap-1.5 rounded-full border border-border bg-background/95 px-3 py-1.5 text-xs font-medium text-muted-foreground shadow-md backdrop-blur transition-colors hover:text-foreground"
          >
            You are viewing older messages
            <ArrowDown className="size-3.5" />
          </button>
        </div>
      )}
    </div>
  );
}
