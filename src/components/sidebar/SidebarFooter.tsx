import { useEffect, useState } from "react";
import { fetchAppInfo } from "../../lib/ipc";

export function SidebarFooter() {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const info = await fetchAppInfo();
        if (active) {
          setVersion(info.version);
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

  return (
    <div className="shrink-0 border-t border-zinc-800/80 px-4 py-2.5 text-[11px] tabular-nums text-zinc-500">
      RAPIDS{version !== null ? ` ${version}` : ""}
    </div>
  );
}
