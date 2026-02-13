import { memo } from "react";
import type { Value } from "platejs";
import { usePlateEditor } from "platejs/react";
import { EditorView } from "@/components/ui/editor";
import { MessageKit } from "../editor/message-kit";

/**
 * Parse content string — returns Plate Value for rich text, null for plain text.
 */
function parseContent(content: string): Value | null {
  if (!content.startsWith("[")) return null;

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
    // Not JSON — plain text
  }

  return null;
}

interface RichTextContentProps {
  content: string | null | undefined;
}

/**
 * Renders message content.
 *
 * - Plain text → simple <span>, no Plate overhead
 * - Rich text (JSON) → Plate static renderer
 * - Empty → placeholder
 */
export const RichTextContent = memo(function RichTextContent({
  content,
}: RichTextContentProps) {
  if (!content) {
    return (
      <span className="italic text-muted-foreground">[empty message]</span>
    );
  }

  const value = parseContent(content);

  // Fast path: plain text — no Plate editor needed
  if (!value) {
    return (
      <span className="whitespace-pre-wrap wrap-break-word">{content}</span>
    );
  }

  return <RichContent value={value} />;
});

// ---------------------------------------------------------------------------
// Rich content: only created for messages with formatting (JSON content)
// ---------------------------------------------------------------------------

const RichContent = memo(function RichContent({ value }: { value: Value }) {
  const editor = usePlateEditor({
    plugins: MessageKit,
    value,
  });

  return <EditorView editor={editor} variant="none" className="text-sm" />;
});
