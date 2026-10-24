import { useEffect, useState } from "react";
import { formatSpeed } from "../../lib/format";
import { fetchAppInfo } from "../../lib/ipc";
import type { AppInfo } from "../../types";
import { cn } from "../../lib/cn";

interface SidebarFooterProps {
  readonly collapsed: boolean;
  readonly active: number;
  readonly waiting: number;
  readonly speed: number;
  readonly queuePaused: boolean;
}

export function SidebarFooter({
  collapsed,
  active,
  waiting,
  speed,
  queuePaused,
}: SidebarFooterProps) {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    let mounted = true;
    void (async () => {
      try {
        const packageInfo = await fetchAppInfo();
        if (mounted) {
          setInfo(packageInfo);
        }
      } catch {
        // The bridge only answers inside the desktop shell; outside it the
        // footer simply shows the product name without a version.
      }
    })();
    return () => {
      mounted = false;
    };
  }, []);

  const label = info !== null ? `${info.name} ${info.version}` : "RAPIDS";
  const summary = `${active} active · ${waiting} waiting`;
  const state = queuePaused ? "Queue paused" : active > 0 ? "Downloading" : "Queue ready";

  if (collapsed) {
    return (
      <div
        className="flex shrink-0 justify-center border-t border-line/80 py-3"
        title={`${state} · ${summary}${speed > 0 ? ` · ${formatSpeed(speed)}` : ""}`}
      >
        <span
          className={cn(
            "h-2 w-2 rounded-full",
            queuePaused ? "bg-amber-400" : active > 0 ? "bg-accent-bright" : "bg-zinc-600",
          )}
        />
      </div>
    );
  }

  return (
    <div className="shrink-0 border-t border-line/80 px-3 py-3">
      <div className="flex items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5 text-[10.5px] text-zinc-400">
          <span
            className={cn(
              "h-1.5 w-1.5 shrink-0 rounded-full",
              queuePaused
                ? "bg-amber-400"
                : active > 0
                  ? "bg-accent-bright shadow-[0_0_8px_rgba(98,228,244,0.85)]"
                  : "bg-zinc-600",
            )}
          />
          <span className="truncate">{state}</span>
        </span>
        <span className="shrink-0 text-[10.5px] tabular-nums text-zinc-500">
          {speed > 0 ? formatSpeed(speed) : "Idle"}
        </span>
      </div>
      <div className="mt-1.5 flex items-center justify-between gap-2 text-[10px] text-zinc-600">
        <span className="truncate">{summary}</span>
        <span className="shrink-0 tabular-nums">{label}</span>
      </div>
    </div>
  );
}
