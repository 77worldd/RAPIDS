import type { NavItem } from "../../config/navigation";
import type { ViewId } from "../../types";
import { SidebarItem } from "./SidebarItem";

interface SidebarSectionProps {
  readonly title: string;
  readonly items: readonly NavItem[];
  readonly collapsed: boolean;
  readonly currentViewId: ViewId;
  readonly counts: ReadonlyMap<ViewId, number>;
  readonly onSelect: (viewId: ViewId) => void;
}

export function SidebarSection({
  title,
  items,
  collapsed,
  currentViewId,
  counts,
  onSelect,
}: SidebarSectionProps) {
  return (
    <section className="px-2.5 py-1">
      {!collapsed && (
        <h3 className="px-2.5 pb-1.5 pt-2 text-[9.5px] font-semibold uppercase tracking-[0.13em] text-zinc-600">
          {title}
        </h3>
      )}
      <div className="space-y-0.5">
        {items.map((item) => (
          <SidebarItem
            key={item.id}
            item={item}
            active={item.id === currentViewId}
            count={counts.get(item.id) ?? 0}
            collapsed={collapsed}
            onSelect={() => onSelect(item.id)}
          />
        ))}
      </div>
    </section>
  );
}
