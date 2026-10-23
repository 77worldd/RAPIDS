import { AddDownloadButton } from "./AddDownloadButton";
import { InspectPageButton } from "./InspectPageButton";
import { PasteUrlButton } from "./PasteUrlButton";
import { QueueToggle } from "./QueueToggle";
import { SearchBox } from "./SearchBox";
import { SettingsButton } from "./SettingsButton";
import { ViewTitle } from "./ViewTitle";

export function TopBar() {
  return (
    <header className="flex h-14 shrink-0 items-center gap-4 border-b border-line/80 bg-panel/75 px-4 backdrop-blur">
      <ViewTitle />
      <div className="ml-auto flex min-w-0 shrink items-center justify-end gap-1.5">
        <SearchBox />
        <div className="mx-1 h-5 w-px shrink-0 bg-line" />
        <PasteUrlButton />
        <AddDownloadButton />
        <QueueToggle />
        <div className="mx-1 h-5 w-px shrink-0 bg-line" />
        <SettingsButton />
        <div className="hidden items-center gap-1.5 min-[1080px]:flex">
          <InspectPageButton />
        </div>
      </div>
    </header>
  );
}
