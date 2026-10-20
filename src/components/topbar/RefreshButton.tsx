import { RefreshCw } from "lucide-react";
import { useStoreDispatch } from "../../state/StoreProvider";
import { IconButton } from "../ui/IconButton";

export function RefreshButton() {
  const dispatch = useStoreDispatch();
  return (
    <IconButton
      label="Refresh"
      onClick={() => dispatch({ type: "hydrate/start" })}
    >
      <RefreshCw size={14} strokeWidth={1.75} />
    </IconButton>
  );
}
