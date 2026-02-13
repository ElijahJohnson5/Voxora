import { useCallback } from "react";
import type { Value } from "platejs";
import { Plate, usePlateEditor } from "platejs/react";
import { Editor, EditorContainer } from "@/components/ui/editor";
import { useMessageStore } from "@/stores/messages";
import { MessageKit } from "../editor/message-kit";

const EMPTY_VALUE: Value = [{ type: "p", children: [{ text: "" }] }];

/** Check if the editor value is effectively empty. */
function isValueEmpty(value: Value): boolean {
  if (value.length === 0) return true;
  if (value.length === 1) {
    const node = value[0];
    if (
      node &&
      "children" in node &&
      Array.isArray(node.children) &&
      node.children.length === 1
    ) {
      const child = node.children[0];
      if (child && "text" in child && typeof child.text === "string") {
        return child.text.trim() === "";
      }
    }
  }
  return false;
}

/**
 * Serialize Plate value to a content string for storage.
 * - If the value is a single paragraph with only plain text, store as plain text.
 * - Otherwise, store as JSON.
 */
function serializeContent(value: Value): string {
  // Check if it's a single paragraph with only plain text (no marks)
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

interface MessageInputProps {
  channelId: string;
  placeholder?: string;
}

export function MessageInput({
  channelId,
  placeholder = "Type a message…",
}: MessageInputProps) {
  const sendMessage = useMessageStore((s) => s.sendMessage);

  const editor = usePlateEditor({
    id: `message-input`,
    plugins: MessageKit,
    value: EMPTY_VALUE,
  });

  const handleSend = useCallback(() => {
    const value = editor.children as Value;
    if (isValueEmpty(value)) return;

    const content = serializeContent(value);
    sendMessage(channelId, content);

    editor.tf.reset();
  }, [editor, channelId, sendMessage]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        handleSend();
      } else if (e.key === "Enter" && e.shiftKey) {
        e.preventDefault();
        editor.tf.insertBreak();
      }
    },
    [handleSend, editor],
  );

  return (
    <div className="border-t border-border px-4 py-3">
      <div className="rounded-lg border border-input bg-background">
        <Plate editor={editor}>
          <EditorContainer variant="comment">
            <Editor
              variant="comment"
              placeholder={placeholder}
              onKeyDown={handleKeyDown}
              autoFocus
              className="min-h-10 max-h-50 overflow-y-auto px-3 py-2 text-sm"
            />
          </EditorContainer>
        </Plate>
        <div className="flex items-center justify-between border-t border-border/50 px-3 py-1">
          <span className="text-xs text-muted-foreground">
            <kbd className="rounded border border-border px-1 font-mono text-[10px]">
              Enter
            </kbd>{" "}
            to send
            <kbd className="rounded border border-border px-1 font-mono text-[10px]">
              Shift + Enter
            </kbd>{" "}
            for new line
          </span>
        </div>
      </div>
    </div>
  );
}
