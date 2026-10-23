import { cn } from "../../lib/cn";

interface ToggleProps {
  readonly checked: boolean;
  readonly onChange: (checked: boolean) => void;
  readonly label: string;
  readonly disabled?: boolean;
}

export function Toggle({
  checked,
  onChange,
  label,
  disabled = false,
}: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border transition-colors duration-150 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/45 focus-visible:ring-offset-2 focus-visible:ring-offset-panel disabled:opacity-40",
        checked ? "border-ink/40 bg-ink" : "border-line bg-line-strong",
      )}
    >
      <span
        className={cn(
          "block h-3.5 w-3.5 rounded-full bg-canvas ring-1 ring-line-strong shadow-sm transition-transform duration-150 ease-out",
          checked ? "translate-x-[1.15rem]" : "translate-x-[0.15rem]",
        )}
      />
    </button>
  );
}
