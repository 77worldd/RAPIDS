import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { IconButton } from "../ui/IconButton";

interface SidebarToggleProps {
  readonly collapsed: boolean;
  readonly onToggle: () => void;
}

export function SidebarToggle({ collapsed, onToggle }: SidebarToggleProps) {
  return (
    <IconButton
      label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      onClick={onToggle}
      className="h-8 w-8"
    >
      {collapsed ? (
        <PanelLeftOpen size={15} strokeWidth={1.8} />
      ) : (
        <PanelLeftClose size={15} strokeWidth={1.8} />
      )}
    </IconButton>
  );
}
