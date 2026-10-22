import { AddDownloadButton } from "./AddDownloadButton";
import { QueueToggle } from "./QueueToggle";
import { RefreshButton } from "./RefreshButton";
import { SearchBox } from "./SearchBox";
import { ViewTitle } from "./ViewTitle";

export function TopBar() {
  return (
    <header className="flex h-12 shrink-0 items-center gap-4 border-b border-zinc-800/80 bg-zinc-950 px-4">
      <ViewTitle />
      <SearchBox />
      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        <AddDownloadButton />
        <QueueToggle />
        <RefreshButton />
      </div>
    </header>
  );
}
