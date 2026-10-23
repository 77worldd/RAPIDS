import {
  CircleCheckBig,
  CircleX,
  Clock,
  Download,
  List,
  Pause,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { ViewId } from "../types";
import { CATEGORIES } from "./categories";

export interface NavItem {
  readonly id: ViewId;
  readonly label: string;
  readonly icon: LucideIcon;
}

export interface NavSection {
  readonly id: "library" | "categories";
  readonly title: string;
  readonly items: readonly NavItem[];
}

export const NAV_SECTIONS: readonly NavSection[] = [
  {
    id: "library",
    title: "Library",
    items: [
      { id: "all", label: "All Downloads", icon: List },
      { id: "downloading", label: "Downloading", icon: Download },
      { id: "queued", label: "Queued", icon: Clock },
      { id: "completed", label: "Completed", icon: CircleCheckBig },
      { id: "paused", label: "Paused", icon: Pause },
      { id: "failed", label: "Failed", icon: CircleX },
    ],
  },
  {
    id: "categories",
    title: "Categories",
    items: CATEGORIES.map((category) => ({
      id: category.id,
      label: category.label,
      icon: category.icon,
    })),
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
