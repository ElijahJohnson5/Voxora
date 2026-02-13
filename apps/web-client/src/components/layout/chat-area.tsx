const placeholderMessages = [
  {
    id: "1",
    author: "Alice",
    content: "Hey everyone! Welcome to Voxora.",
    time: "12:00 PM",
  },
  {
    id: "2",
    author: "Bob",
    content: "Thanks! Excited to be here.",
    time: "12:01 PM",
  },
  {
    id: "3",
    author: "Charlie",
    content: "This is looking great so far!",
    time: "12:03 PM",
  },
];

export function ChatArea() {
  return (
    <div className="flex h-full flex-col">
      {/* Messages */}
      <div className="flex-1 space-y-4 overflow-y-auto p-4">
        {placeholderMessages.map((msg) => (
          <div key={msg.id} className="flex gap-3">
            <div className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full bg-primary text-xs font-medium text-primary-foreground">
              {msg.author[0]}
            </div>
            <div>
              <div className="flex items-baseline gap-2">
                <span className="text-sm font-semibold">{msg.author}</span>
                <span className="text-xs text-muted-foreground">
                  {msg.time}
                </span>
              </div>
              <p className="text-sm">{msg.content}</p>
            </div>
          </div>
        ))}
      </div>

      {/* Message input */}
      <div className="border-t border-border p-4">
        <div className="rounded-md border border-input bg-secondary px-4 py-2.5 text-sm text-muted-foreground">
          Message #general
        </div>
      </div>
    </div>
  );
}
