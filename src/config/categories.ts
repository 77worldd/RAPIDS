import type { TagId } from "../types";

export interface Category {
  readonly id: TagId;
  readonly label: string;
}

export const CATEGORIES: readonly Category[] = [
  { id: "apps", label: "Apps" },
  { id: "movies", label: "Movies" },
  { id: "music", label: "Music" },
  { id: "images", label: "Images" },
  { id: "other", label: "Other" },
];
