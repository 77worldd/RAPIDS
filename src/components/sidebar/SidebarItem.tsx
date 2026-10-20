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
        "relative flex h-8 w-full items-center gap-2.5 rounded-md pl-2.5 pr-2 text-left transition-colors duration-150 ease-out focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-500/70",
        collapsed && "justify-center pl-0 pr-0",
        active
          ? "bg-zinc-900 text-zinc-50"
          : "text-zinc-400 hover:bg-zinc-900/60 hover:text-zinc-200",
      )}
    >
      {active && (
        <motion.span
          layoutId="sidebar-active-marker"
          transition={TRANSITION}
          className="absolute inset-y-1 left-0 w-0.5 rounded-full bg-emerald-500"
        />
      )}
      <Icon
        size={15}
        strokeWidth={1.75}
        className={cn("shrink-0", active && "text-emerald-400")}
      />
      <span className={cn("flex-1 truncate", collapsed && "sr-only")}>
        {item.label}
      </span>
      <span className={cn("ml-auto", collapsed && "sr-only")}>
        <CountBadge count={count} active={active} />
      </span>
    </button>
  );
}
