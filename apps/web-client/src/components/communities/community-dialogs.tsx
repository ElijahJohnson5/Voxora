import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Label } from "@/components/ui/label";

export interface PodOption {
  podId: string;
  podName: string;
}

export function CreateCommunityDialog({
  open,
  onOpenChange,
  pods,
  onCreate,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  pods: PodOption[];
  onCreate: (podId: string, name: string, description?: string) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [selectedPod, setSelectedPod] = useState<string>("");
  const [submitting, setSubmitting] = useState(false);

  const effectivePod = selectedPod || pods[0]?.podId || "";

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!name.trim() || !effectivePod) return;
    setSubmitting(true);
    await onCreate(effectivePod, name.trim(), description.trim() || undefined);
    setSubmitting(false);
    setName("");
    setDescription("");
    setSelectedPod("");
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Create Community</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            {pods.length > 1 && (
              <div className="space-y-2">
                <Label>Pod</Label>
                <Select value={effectivePod} onValueChange={setSelectedPod}>
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Select a pod" />
                  </SelectTrigger>
                  <SelectContent>
                    {pods.map((p) => (
                      <SelectItem key={p.podId} value={p.podId}>
                        {p.podName}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}
            <Input
              placeholder="Community name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
            />
            <Input
              placeholder="Description (optional)"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>
          <DialogFooter>
            <Button type="submit" disabled={!name.trim() || !effectivePod || submitting}>
              {submitting ? "Creating..." : "Create"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function JoinInviteDialog({
  open,
  onOpenChange,
  pods,
  onJoin,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  pods: PodOption[];
  onJoin: (podId: string, code: string) => Promise<void>;
}) {
  const [code, setCode] = useState("");
  const [selectedPod, setSelectedPod] = useState<string>("");
  const [submitting, setSubmitting] = useState(false);

  const effectivePod = selectedPod || pods[0]?.podId || "";

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!code.trim() || !effectivePod) return;
    setSubmitting(true);
    await onJoin(effectivePod, code.trim());
    setSubmitting(false);
    setCode("");
    setSelectedPod("");
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Join via Invite</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            {pods.length > 1 && (
              <div className="space-y-2">
                <Label>Pod</Label>
                <Select value={effectivePod} onValueChange={setSelectedPod}>
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Select a pod" />
                  </SelectTrigger>
                  <SelectContent>
                    {pods.map((p) => (
                      <SelectItem key={p.podId} value={p.podId}>
                        {p.podName}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}
            <Input
              placeholder="Invite code"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button type="submit" disabled={!code.trim() || !effectivePod || submitting}>
              {submitting ? "Joining..." : "Join"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
