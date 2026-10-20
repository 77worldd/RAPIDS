import { useEffect, useState } from "react";
import { fetchAppInfo } from "../../lib/ipc";
import type { AppInfo } from "../../types";

export function SidebarFooter() {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const packageInfo = await fetchAppInfo();
        if (active) {
          setInfo(packageInfo);
        }
      } catch {
        // The bridge only answers inside the desktop shell; outside it the
        // footer simply shows the product name without a version.
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  const label = info !== null ? `${info.name} ${info.version}` : "RAPIDS";
  return (
    <div className="shrink-0 border-t border-zinc-800/80 px-4 py-2.5 text-[11px] tabular-nums text-zinc-500">
      {label}
    </div>
  );
}
