import { motion } from "framer-motion";
import { useMemo } from "react";
import markUrl from "../../assets/rapids-mark.svg";
import { NAV_SECTIONS } from "../../config/navigation";
import { cn } from "../../lib/cn";
import { countForView } from "../../state/selectors";
import { useStoreDispatch, useStoreState } from "../../state/StoreProvider";
import type { ViewId } from "../../types";
import { TRANSITION } from "../motion/variants";
import { SidebarFooter } from "./SidebarFooter";
import { SidebarSection } from "./SidebarSection";
import { SidebarToggle } from "./SidebarToggle";

const EXPANDED_WIDTH = 240;
const COLLAPSED_WIDTH = 56;

export function Sidebar() {
  const {
    tasks,
    viewId,
    sidebarCollapsed: collapsed,
    queuePaused,
  } = useStoreState();
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

  const active = tasks.filter((task) => task.status === "downloading").length;
  const waiting = tasks.filter(
    (task) => task.status === "queued" || task.status === "scheduled",
  ).length;
  const speed = tasks.reduce(
    (total, task) =>
      task.status === "downloading" ? total + task.speedBps : total,
    0,
  );

  return (
    <motion.aside
      initial={false}
      animate={{ width: collapsed ? COLLAPSED_WIDTH : EXPANDED_WIDTH }}
      transition={TRANSITION}
      className="relative z-10 flex h-full shrink-0 flex-col overflow-hidden border-r border-line/80 bg-[#0c0e10]"
    >
      <div
        className={cn(
          "flex h-14 shrink-0 items-center border-b border-line/80 px-3",
          collapsed ? "justify-center" : "gap-2.5",
        )}
      >
        {!collapsed && (
          <>
            <img
              src={markUrl}
              alt=""
              aria-hidden="true"
              draggable={false}
              className="h-8 w-8 shrink-0 select-none"
            />
            <div className="min-w-0 flex-1">
              <p className="text-[12px] font-semibold tracking-[0.14em] text-zinc-100">
                RAPIDS
              </p>
              <p className="mt-0.5 text-[10px] text-zinc-500">Download manager</p>
            </div>
          </>
        )}
        <div className={cn(collapsed && "flex justify-center")}>
          <SidebarToggle
            collapsed={collapsed}
            onToggle={() => dispatch({ type: "sidebar/toggle" })}
          />
        </div>
      </div>

      <nav
        aria-label="Download views"
        className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden py-2"
      >
        {NAV_SECTIONS.map((section, index) => (
          <div key={section.id}>
            {collapsed && index > 0 && (
              <div className="mx-2 my-2 h-px bg-line/80" />
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

      <SidebarFooter
        collapsed={collapsed}
        active={active}
        waiting={waiting}
        speed={speed}
        queuePaused={queuePaused}
      />
    </motion.aside>
  );
}
