import { Plus } from "lucide-react";
import { useState } from "react";
import { AddDownloadDialog } from "../tasks/AddDownloadDialog";
import { Button } from "../ui/Button";

export function AddDownloadButton() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <Button onClick={() => setOpen(true)}>
        <Plus size={14} strokeWidth={2} />
        Add download
      </Button>
      {open && <AddDownloadDialog onClose={() => setOpen(false)} />}
    </>
  );
}
