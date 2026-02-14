import { createContext, use } from "react";

interface ChannelContextValue {
  podId: string;
  channelId: string;
}

const ChannelContext = createContext<ChannelContextValue | null>(null);

export function ChannelProvider({
  podId,
  channelId,
  children,
}: ChannelContextValue & { children: React.ReactNode }) {
  return (
    <ChannelContext value={{ podId, channelId }}>{children}</ChannelContext>
  );
}

export function useChannel(): ChannelContextValue {
  const ctx = use(ChannelContext);
  if (!ctx) {
    throw new Error("useChannel must be used within a <ChannelProvider>");
  }
  return ctx;
}
