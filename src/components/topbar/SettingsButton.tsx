import { Settings2 } from "lucide-react";
import { useState } from "react";
import { SettingsDialog } from "../settings/SettingsDialog";
import { IconButton } from "../ui/IconButton";

export function SettingsButton() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <IconButton label="Settings" onClick={() => setOpen(true)}>
        <Settings2 size={15} strokeWidth={1.8} />
      </IconButton>
      {open && <SettingsDialog onClose={() => setOpen(false)} />}
    </>
  );
}
