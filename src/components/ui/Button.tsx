import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/cn";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly children: ReactNode;
}

export function Button({
  className,
  children,
  type = "button",
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex h-7 items-center justify-center gap-1.5 rounded-md bg-emerald-500 px-3 text-[13px] font-medium text-zinc-950 transition-colors duration-150 ease-out hover:bg-emerald-400 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-400",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}
