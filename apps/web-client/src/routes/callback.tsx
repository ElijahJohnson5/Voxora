import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/callback")({
  component: CallbackPage,
});

function CallbackPage() {
  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="flex flex-col items-center gap-4">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
        <p className="text-sm text-muted-foreground">Processing login...</p>
      </div>
    </div>
  );
}
