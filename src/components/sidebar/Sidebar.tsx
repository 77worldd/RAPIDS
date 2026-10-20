import { motion } from "framer-motion";
import { useMemo } from "react";
import { NAV_SECTIONS } from "../../config/navigation";
import { cn } from "../../lib/cn";
import { countForView } from "../../state/selectors";
import { useStoreDispatch, useStoreState } from "../../state/StoreProvider";
import type { ViewId } from "../../types";
import { TRANSITION } from "../motion/variants";
import { SidebarFooter } from "./SidebarFooter";
import { SidebarSection } from "./SidebarSection";
import { SidebarToggle } from "./SidebarToggle";

const EXPANDED_WIDTH = 232;
const COLLAPSED_WIDTH = 52;

export function Sidebar() {
  const { tasks, viewId, sidebarCollapsed: collapsed } = useStoreState();
  const dispatch = useStoreDispatch();

  const counts = useMemo(() => {
    const map = new Map<ViewId, number>();
    for (const section of NAV_SECTIONS) {
      for (const item of section.items) {
        map.set(item.id, countForView(tasks, item.id));
      }
    }
    return map;
  }, [tasks]);

  return (
    <motion.aside
      initial={false}
      animate={{ width: collapsed ? COLLAPSED_WIDTH : EXPANDED_WIDTH }}
      transition={TRANSITION}
      className="relative z-10 flex h-full shrink-0 flex-col overflow-hidden border-r border-zinc-800/80 bg-zinc-950"
    >
      <div className="flex h-12 shrink-0 items-center gap-2 border-b border-zinc-800/80 px-3">
        <span
          className={cn(
            "text-[13px] font-semibold tracking-wider text-zinc-100",
            collapsed && "sr-only",
          )}
        >
          RAPIDS
        </span>
        <div className={cn(!collapsed && "ml-auto")}>
          <SidebarToggle
            collapsed={collapsed}
            onToggle={() => dispatch({ type: "sidebar/toggle" })}
          />
        </div>
      </div>
      <nav
        aria-label="Download views"
        className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden py-1.5"
      >
        {NAV_SECTIONS.map((section, index) => (
          <div key={section.id}>
            {collapsed && index > 0 && (
              <div className="mx-2 my-1.5 h-px bg-zinc-800" />
            )}
            <SidebarSection
              title={section.title}
              items={section.items}
              collapsed={collapsed}
              currentViewId={viewId}
              counts={counts}
              onSelect={(nextView) =>
                dispatch({ type: "navigate", viewId: nextView })
              }
            />
          </div>
        ))}
      </nav>
      <SidebarFooter />
    </motion.aside>
  );
}
