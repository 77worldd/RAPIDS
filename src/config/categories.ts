import {
  Archive,
  File,
  FileText,
  Film,
  Image,
  Music2,
  Package,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { TagId } from "../types";

export interface Category {
  readonly id: TagId;
  readonly label: string;
  readonly description: string;
  readonly icon: LucideIcon;
}

export const CATEGORIES: readonly Category[] = [
  {
    id: "movies",
    label: "Video",
    description: "Movies, shows, and clips",
    icon: Film,
  },
  {
    id: "music",
    label: "Music",
    description: "Songs, albums, and audio",
    icon: Music2,
  },
  {
    id: "images",
    label: "Images",
    description: "Photos, art, and screenshots",
    icon: Image,
  },
  {
    id: "documents",
    label: "Documents",
    description: "PDFs, books, and office files",
    icon: FileText,
  },
  {
    id: "archives",
    label: "Archives",
    description: "Zip, 7z, tar, and disk images",
    icon: Archive,
  },
  {
    id: "apps",
    label: "Software",
    description: "Installers and app packages",
    icon: Package,
  },
  {
    id: "other",
    label: "Other",
    description: "Everything else",
    icon: File,
  },
];

export function categoryFor(id: TagId): Category {
  const category = CATEGORIES.find((entry) => entry.id === id);
  if (category === undefined) {
    throw new Error(`Unknown download category: ${String(id)}`);
  }
  return category;
}
