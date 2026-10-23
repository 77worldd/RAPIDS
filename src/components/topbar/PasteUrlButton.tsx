import { ClipboardPaste, Loader2 } from "lucide-react";
import { useState } from "react";
import { describeError } from "../../lib/ipc";
import { normalizeClipboardUrl } from "../../lib/fileMeta";
import { readClipboardText } from "../../lib/native";
import { useAddDownload } from "../../state/AddDownloadProvider";
import { useStoreDispatch } from "../../state/StoreProvider";
import { IconButton } from "../ui/IconButton";

export function PasteUrlButton() {
  const { openAdd } = useAddDownload();
  const dispatch = useStoreDispatch();
  const [reading, setReading] = useState(false);

  async function paste(): Promise<void> {
    if (reading) return;
    setReading(true);
    try {
      const url = normalizeClipboardUrl(await readClipboardText());
      if (url === null) {
        dispatch({
          type: "notice/set",
          message: "The clipboard does not contain an HTTP or HTTPS download link.",
        });
        return;
      }
      openAdd(url);
    } catch (cause) {
      dispatch({ type: "notice/set", message: describeError(cause) });
    } finally {
      setReading(false);
    }
  }

  return (
    <IconButton
      label="Paste a link from the clipboard"
      onClick={() => void paste()}
      disabled={reading}
      className="border border-transparent"
    >
      {reading ? (
        <Loader2 size={14} className="animate-spin" />
      ) : (
        <ClipboardPaste size={15} strokeWidth={1.8} />
      )}
    </IconButton>
  );
}
