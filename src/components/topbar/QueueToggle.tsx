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
      variant="secondary"
      onClick={() => {
        command(() => resumeQueue());
      }}
      title="Start queued downloads"
    >
      <Play size={13} strokeWidth={2.2} />
      Start queue
    </Button>
  ) : (
    <Button
      variant="secondary"
      onClick={() => {
        command(() => pauseQueue());
      }}
      title="Stop new downloads from starting"
    >
      <Pause size={13} strokeWidth={2.2} />
      Pause queue
    </Button>
  );
}
