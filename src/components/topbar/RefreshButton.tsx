import { RefreshCw } from "lucide-react";
import { useRefreshTasks } from "../../state/StoreProvider";
import { IconButton } from "../ui/IconButton";

export function RefreshButton() {
  const refresh = useRefreshTasks();
  return (
    <IconButton label="Refresh" onClick={refresh}>
      <RefreshCw size={14} strokeWidth={1.75} />
    </IconButton>
  );
}
