import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_authenticated/")({
  component: HomePage,
});

function HomePage() {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center">
        <h1 className="text-2xl font-bold">Welcome to Voxora</h1>
        <p className="mt-2 text-muted-foreground">
          Select a community to get started
        </p>
      </div>
    </div>
  );
}
