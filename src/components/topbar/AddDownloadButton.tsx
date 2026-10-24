import { Plus } from "lucide-react";
import { useAddDownload } from "../../state/AddDownloadProvider";
import { Button } from "../ui/Button";

export function AddDownloadButton() {
  const { openAdd } = useAddDownload();

  return (
    <Button onClick={() => openAdd()} className="shadow-[0_0_0_1px_rgba(31,185,223,0.16),0_8px_20px_-12px_rgba(31,185,223,0.7)]">
      <Plus size={14} strokeWidth={2.2} />
      Add download
    </Button>
  );
}
