export function cn(...parts: Array<string | false | null | undefined>): string {
  return parts
    .filter((part): part is string => typeof part === "string" && part !== "")
    .join(" ");
}
