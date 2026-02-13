import { type Value } from "platejs";
import { type TPlateEditor, useEditorRef } from "platejs/react";

import { AutoformatKit } from "./plugins/autoformat-kit";
import { BasicBlocksKit } from "./plugins/basic-blocks-kit";
import { BasicMarksKit } from "./plugins/basic-marks-kit";
import { CodeBlockKit } from "./plugins/code-block-kit";
import { CursorOverlayKit } from "./plugins/cursor-overlay-kit";
import { EmojiKit } from "./plugins/emoji-kit";
import { ExitBreakKit } from "./plugins/exit-break-kit";
import { FloatingToolbarKit } from "./plugins/floating-toolbar-kit";
import { LinkKit } from "./plugins/link-kit";
import { ListKit } from "./plugins/list-kit";
import { MarkdownKit } from "./plugins/markdown-kit";
import { MediaKit } from "./plugins/media-kit";
import { MentionKit } from "./plugins/mention-kit";
import { MessageSlashKit } from "./plugins/message-slash-kit";

export const MessageKit = [
  // Elements
  ...BasicBlocksKit,
  ...CodeBlockKit,
  ...MediaKit,
  ...LinkKit,
  ...MentionKit,

  // Marks
  ...BasicMarksKit,

  // Block Style
  ...ListKit,

  // Editing
  ...MessageSlashKit,
  ...AutoformatKit,
  ...CursorOverlayKit,
  ...EmojiKit,
  ...ExitBreakKit,

  // Parsers
  ...MarkdownKit,

  // UI
  ...FloatingToolbarKit,
];

export type MessageEditor = TPlateEditor<Value, (typeof MessageKit)[number]>;

export const useMessageEditor = () => useEditorRef<MessageEditor>();
