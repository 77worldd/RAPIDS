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
    >
      {collapsed ? (
        <PanelLeftOpen size={15} strokeWidth={1.75} />
      ) : (
        <PanelLeftClose size={15} strokeWidth={1.75} />
      )}
    </IconButton>
  );
}
