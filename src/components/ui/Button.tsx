import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/cn";

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
type ButtonSize = "sm" | "md";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly children: ReactNode;
  readonly variant?: ButtonVariant;
  readonly size?: ButtonSize;
}

const VARIANTS: Record<ButtonVariant, string> = {
  primary:
    "bg-accent text-zinc-950 shadow-[inset_0_1px_0_rgba(255,255,255,0.14)] hover:bg-accent-bright active:bg-accent",
  secondary:
    "border border-line bg-elevated/70 text-zinc-200 hover:border-zinc-600 hover:bg-elevated hover:text-zinc-50",
  ghost: "text-zinc-400 hover:bg-white/[0.06] hover:text-zinc-100",
  danger:
    "border border-red-900/70 bg-red-950/30 text-red-200 hover:border-red-800 hover:bg-red-950/60",
};

const SIZES: Record<ButtonSize, string> = {
  sm: "h-7 px-2.5 text-[12px]",
  md: "h-8 px-3 text-[12.5px]",
};

export function Button({
  className,
  children,
  type = "button",
  variant = "primary",
  size = "md",
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md font-medium transition-[background-color,border-color,color,box-shadow] duration-150 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/45 focus-visible:ring-offset-2 focus-visible:ring-offset-panel disabled:pointer-events-none disabled:opacity-45",
        VARIANTS[variant],
        SIZES[size],
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}
