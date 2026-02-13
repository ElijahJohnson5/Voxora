import { memo, useMemo, useCallback } from "react";
import type { Value } from "platejs";
import { Plate, usePlateEditor } from "platejs/react";
import type { Message } from "@/stores/messages";
import { useMessageStore, type Reaction } from "@/stores/messages";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Editor, EditorContainer } from "@/components/ui/editor";
import { MessageKit } from "@/components/editor/message-kit";
import { Pencil, Trash2, SmilePlus } from "lucide-react";
import { cn } from "@/lib/utils";
import { MessageReactions } from "./reactions";
import { RichTextContent } from "./rich-text-content";
import type React from "react";

interface MessageItemProps {
  message: Message;
  isOwn: boolean;
  compact: boolean;
  pending?: boolean;
  editing?: boolean;
  onEditStart?: (messageId: string) => void;
  onEditEnd?: () => void;
}

function formatTimestamp(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const isToday = date.toDateString() === now.toDateString();

  const time = date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });

  if (isToday) return `Today at ${time}`;

  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString())
    return `Yesterday at ${time}`;

  return `${date.toLocaleDateString()} ${time}`;
}

function getInitials(id: string): string {
  return id.slice(0, 2).toUpperCase();
}

export const MessageItem = memo(function MessageItem({
  message,
  isOwn,
  compact,
  pending = false,
  editing = false,
  onEditStart,
  onEditEnd,
}: MessageItemProps) {
  const editMessage = useMessageStore((s) => s.editMessage);
  const deleteMessage = useMessageStore((s) => s.deleteMessage);
  const reactions: Reaction[] =
    useMessageStore((s) => s.reactions[message.id]) ?? [];

  const authorDisplay = useMemo(() => {
    return message.author_id.slice(0, 8);
  }, [message.author_id]);

  const handleSaveEdit = useCallback(
    async (content: string) => {
      if (!content.trim()) return;
      await editMessage(message.channel_id, message.id, content);
      onEditEnd?.();
    },
    [editMessage, message.channel_id, message.id, onEditEnd],
  );

  const handleCancelEdit = useCallback(() => {
    onEditEnd?.();
  }, [onEditEnd]);

  const handleDelete = useCallback(() => {
    deleteMessage(message.channel_id, message.id);
  }, [deleteMessage, message.channel_id, message.id]);

  const handleEditStart = useCallback(() => {
    onEditStart?.(message.id);
  }, [onEditStart, message.id]);

  if (compact) {
    return (
      <div
        className={cn(
          "group relative flex items-start gap-3 py-0.5 pl-13 pr-4 hover:bg-accent/50",
          pending && "opacity-50",
        )}
      >
        {/* Compact timestamp on hover */}
        <span className="absolute left-0 top-1/2 hidden w-13 -translate-y-1/2 text-center text-[10px] text-muted-foreground group-hover:inline">
          {new Date(message.created_at).toLocaleTimeString([], {
            hour: "2-digit",
            minute: "2-digit",
          })}
        </span>

        {/* Content */}
        <div className="min-w-0 flex-1">
          {editing ? (
            <EditForm
              content={message.content}
              onSave={handleSaveEdit}
              onCancel={handleCancelEdit}
            />
          ) : (
            <div className="text-sm">
              <RichTextContent content={message.content} />
              {message.edited_at && (
                <span className="ml-1 text-[10px] text-muted-foreground">
                  (edited)
                </span>
              )}
            </div>
          )}
          {reactions.length > 0 && (
            <MessageReactions
              reactions={reactions}
              channelId={message.channel_id}
              messageId={message.id}
            />
          )}
        </div>

        {/* Action buttons — CSS-only hover, no state re-renders */}
        {isOwn && !pending && !editing && (
          <MessageActions
            onEdit={handleEditStart}
            onDelete={handleDelete}
          />
        )}
        {!isOwn && !pending && (
          <ReactionButton
            channelId={message.channel_id}
            messageId={message.id}
          />
        )}
      </div>
    );
  }

  return (
    <div
      className={cn(
        "group relative mt-1 flex items-start gap-3 py-1 pr-4 hover:bg-accent/50",
        pending && "opacity-50",
      )}
    >
      {/* Avatar */}
      <Avatar className="mt-0.5 size-10 shrink-0">
        <AvatarFallback className="text-xs">
          {getInitials(message.author_id)}
        </AvatarFallback>
      </Avatar>

      {/* Message body */}
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex items-baseline gap-2">
          <span className="text-sm font-semibold">{authorDisplay}</span>
          <span className="text-xs text-muted-foreground">
            {formatTimestamp(message.created_at)}
          </span>
          {message.edited_at && (
            <Badge variant="outline" className="h-4 px-1 text-[10px]">
              edited
            </Badge>
          )}
        </div>

        {editing ? (
          <EditForm
            content={message.content}
            onSave={handleSaveEdit}
            onCancel={handleCancelEdit}
          />
        ) : (
          <div className="text-sm">
            <RichTextContent content={message.content} />
          </div>
        )}

        {reactions.length > 0 && (
          <MessageReactions
            reactions={reactions}
            channelId={message.channel_id}
            messageId={message.id}
          />
        )}
      </div>

      {/* Action buttons — CSS-only hover, no state re-renders */}
      {isOwn && !pending && !editing && (
        <MessageActions
          onEdit={handleEditStart}
          onDelete={handleDelete}
        />
      )}
      {!pending && (
        <ReactionButton channelId={message.channel_id} messageId={message.id} />
      )}
    </div>
  );
});

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function parseEditValue(content: string | null | undefined): Value {
  if (!content) return [{ type: "p", children: [{ text: "" }] }];

  try {
    const value = JSON.parse(content) as Value;
    if (
      Array.isArray(value) &&
      value.length > 0 &&
      typeof value[0] === "object"
    ) {
      return value;
    }
  } catch {
    // plain text
  }

  return [{ type: "p", children: [{ text: content }] }];
}

function serializeEditValue(value: Value): string {
  if (value.length === 1) {
    const node = value[0];
    if (
      node &&
      "type" in node &&
      (node.type === "p" || !node.type) &&
      "children" in node &&
      Array.isArray(node.children)
    ) {
      const allPlainText = node.children.every(
        (child: Record<string, unknown>) => {
          if (!("text" in child)) return false;
          const keys = Object.keys(child);
          return keys.length === 1 && keys[0] === "text";
        },
      );
      if (allPlainText) {
        return node.children
          .map((child: Record<string, unknown>) => child.text as string)
          .join("");
      }
    }
  }

  return JSON.stringify(value);
}

function EditForm({
  content,
  onSave,
  onCancel,
}: {
  content: string | null | undefined;
  onSave: (content: string) => void;
  onCancel: () => void;
}) {
  const editor = usePlateEditor({
    plugins: MessageKit,
    value: parseEditValue(content),
  });

  const handleSave = useCallback(() => {
    const value = editor.children as Value;
    const serialized = serializeEditValue(value);
    onSave(serialized);
  }, [editor, onSave]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSave();
      }
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      }
    },
    [handleSave, onCancel],
  );

  return (
    <div className="mt-1">
      <div className="rounded-md border border-input bg-background">
        <Plate editor={editor}>
          <EditorContainer variant="comment">
            <Editor
              variant="comment"
              autoFocus
              onKeyDown={handleKeyDown}
              className="min-h-10 max-h-50 overflow-y-auto px-3 py-2 text-sm"
            />
          </EditorContainer>
        </Plate>
      </div>
      <div className="mt-1 flex gap-1 text-xs text-muted-foreground">
        <span>
          <kbd className="rounded border border-border px-1 font-mono text-[10px]">
            Esc
          </kbd>{" "}
          to{" "}
          <button
            className="text-primary underline"
            onClick={onCancel}
            type="button"
          >
            cancel
          </button>
        </span>
        <span>•</span>
        <span>
          <kbd className="rounded border border-border px-1 font-mono text-[10px]">
            Enter
          </kbd>{" "}
          to{" "}
          <button
            className="text-primary underline"
            onClick={handleSave}
            type="button"
          >
            save
          </button>
        </span>
      </div>
    </div>
  );
}

function MessageActions({
  onEdit,
  onDelete,
}: {
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="absolute -top-3 right-2 hidden rounded-md border border-border bg-background shadow-sm group-hover:flex">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="size-7"
            onClick={onEdit}
          >
            <Pencil className="size-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Edit</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-destructive hover:text-destructive"
            onClick={onDelete}
          >
            <Trash2 className="size-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Delete</TooltipContent>
      </Tooltip>
    </div>
  );
}

function ReactionButton({
  channelId: _channelId,
  messageId: _messageId,
}: {
  channelId: string;
  messageId: string;
}) {
  // TODO: implement full emoji picker popover
  return (
    <div className="absolute -top-3 right-2 hidden rounded-md border border-border bg-background shadow-sm group-hover:flex">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="ghost" size="icon" className="size-7">
            <SmilePlus className="size-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Add Reaction</TooltipContent>
      </Tooltip>
    </div>
  );
}
