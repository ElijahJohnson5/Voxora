"use client";

import { SlashInputPlugin, SlashPlugin } from "@platejs/slash-command/react";
import { KEYS } from "platejs";

import { MessageSlashInputElement } from "@/components/ui/message-slash-node";

export const MessageSlashKit = [
  SlashPlugin.configure({
    options: {
      triggerQuery: (editor) =>
        !editor.api.some({
          match: { type: editor.getType(KEYS.codeBlock) },
        }),
    },
  }),
  SlashInputPlugin.withComponent(MessageSlashInputElement),
];
