import {
  CheckCheck,
  CircleCheckBig,
  CircleDashed,
  Clock,
  Disc3,
  Download,
  File,
  Film,
  Image,
  List,
  Package,
  Pause,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { ViewId } from "../types";

export interface NavItem {
  readonly id: ViewId;
  readonly label: string;
  readonly icon: LucideIcon;
}

export interface NavSection {
  readonly id: "tasks" | "scheduled" | "tags";
  readonly title: string;
  readonly items: readonly NavItem[];
}

export const NAV_SECTIONS: readonly NavSection[] = [
  {
    id: "tasks",
    title: "Tasks",
    items: [
      { id: "all", label: "All", icon: List },
      { id: "running", label: "Running", icon: Download },
      { id: "paused", label: "Paused", icon: Pause },
      { id: "completed", label: "Completed", icon: CircleCheckBig },
      { id: "unfinished", label: "Unfinished", icon: CircleDashed },
    ],
  },
  {
    id: "scheduled",
    title: "Scheduled",
    items: [
      { id: "waiting", label: "Waiting", icon: Clock },
      { id: "finished", label: "Finished", icon: CheckCheck },
    ],
  },
  {
    id: "tags",
    title: "Tags",
    items: [
      { id: "apps", label: "Apps", icon: Package },
      { id: "movies", label: "Movies", icon: Film },
      { id: "music", label: "Music", icon: Disc3 },
      { id: "images", label: "Images", icon: Image },
      { id: "other", label: "Other", icon: File },
    ],
  },
];

export function viewLabel(id: ViewId): string {
  for (const section of NAV_SECTIONS) {
    const match = section.items.find((item) => item.id === id);
    if (match !== undefined) {
      return match.label;
    }
  }
  throw new Error(`Unknown view: ${String(id)}`);
}
