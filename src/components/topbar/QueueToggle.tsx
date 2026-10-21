import { Pause, Play } from "lucide-react";
import { pauseQueue, resumeQueue } from "../../lib/ipc";
import { useCommand, useStoreState } from "../../state/StoreProvider";
import { Button } from "../ui/Button";

/** The queue switch gates new starts; it does not stop active downloads. */
export function QueueToggle() {
  const { status, queuePaused } = useStoreState();
  const command = useCommand();
  if (status !== "ready") return null;

  return queuePaused ? (
    <Button
      onClick={() => {
        command(() => resumeQueue());
      }}
    >
      <Play size={13} strokeWidth={2} />
      Resume queue
    </Button>
  ) : (
    <Button
      className="bg-zinc-800 text-zinc-200 hover:bg-zinc-700"
      onClick={() => {
        command(() => pauseQueue());
      }}
    >
      <Pause size={13} strokeWidth={2} />
      Pause queue
    </Button>
  );
}
