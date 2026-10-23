import { motion } from "framer-motion";
import type { NavItem } from "../../config/navigation";
import { cn } from "../../lib/cn";
import { TRANSITION } from "../motion/variants";
import { CountBadge } from "./CountBadge";

interface SidebarItemProps {
  readonly item: NavItem;
  readonly active: boolean;
  readonly count: number;
  readonly collapsed: boolean;
  readonly onSelect: () => void;
}

export function SidebarItem({
  item,
  active,
  count,
  collapsed,
  onSelect,
}: SidebarItemProps) {
  const Icon = item.icon;
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={active ? "page" : undefined}
      {...(collapsed ? { title: item.label } : {})}
      className={cn(
        "relative flex h-8 w-full items-center gap-2.5 rounded-md pl-2.5 pr-2 text-left outline-none transition-colors duration-150 ease-out focus-visible:ring-1 focus-visible:ring-ink/65",
        collapsed && "justify-center pl-0 pr-0",
        active
          ? "bg-ink/[0.09] text-ink"
          : "text-muted hover:bg-ink/[0.035] hover:text-ink",
      )}
    >
      {active && (
        <motion.span
          layoutId="sidebar-active-marker"
          transition={TRANSITION}
          className="absolute inset-y-1.5 left-0 w-0.5 rounded-full bg-ink"
        />
      )}
      <Icon
        size={15}
        strokeWidth={1.75}
        className={cn(
          "shrink-0 transition-colors",
          active ? "text-ink-soft" : "text-muted",
        )}
      />
      <span
        className={cn("flex-1 truncate text-[12px]", collapsed && "sr-only")}
      >
        {item.label}
      </span>
      <span className={cn("ml-auto", collapsed && "sr-only")}>
        <CountBadge count={count} active={active} />
      </span>
    </button>
  );
}
