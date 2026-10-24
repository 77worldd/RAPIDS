import { Film, X } from "lucide-react";
import { sniffCandidateLabel } from "../../lib/sniff";
import type { SniffCandidate } from "../../types";
import { Button } from "../ui/Button";
import { IconButton } from "../ui/IconButton";

const SOURCE_LABELS: Record<SniffCandidate["source"], string> = {
  video: "Video",
  videoSource: "Video source",
  link: "Direct link",
  navigation: "Direct address",
};

interface MediaCandidatePopupProps {
  readonly pageUrl: string;
  readonly candidates: readonly SniffCandidate[];
  readonly onSelect: (candidate: SniffCandidate) => void;
  readonly onDismiss: () => void;
}

export function MediaCandidatePopup({
  pageUrl,
  candidates,
  onSelect,
  onDismiss,
}: MediaCandidatePopupProps) {
  const pageHost = new URL(pageUrl).host;

  return (
    <aside
      aria-labelledby="media-candidates-title"
      className="fixed right-4 bottom-4 z-50 w-[min(25rem,calc(100vw-2rem))] max-h-[70vh] overflow-hidden rounded-xl border border-zinc-700 bg-zinc-950 text-zinc-100 shadow-2xl"
    >
      <div className="flex items-start gap-2 border-b border-zinc-800 px-4 py-3">
        <div className="min-w-0 flex-1">
          <h2 id="media-candidates-title" className="text-[13px] font-semibold">
            Media found
          </h2>
          <p className="truncate text-[11px] text-zinc-400">On {pageHost}</p>
        </div>
        <IconButton label="Dismiss media suggestions" onClick={onDismiss}>
          <X size={15} strokeWidth={1.75} />
        </IconButton>
      </div>
      <p className="px-4 pt-3 text-[12px] leading-relaxed text-zinc-400">
        Review a URL in the add form. Nothing is downloaded until you confirm.
      </p>
      <ul className="max-h-[min(40vh,19rem)] space-y-1 overflow-y-auto p-3">
        {candidates.map((candidate) => {
          const label = sniffCandidateLabel(candidate);
          return (
            <li
              key={candidate.url}
              className="flex items-center gap-2 rounded-lg border border-zinc-800 bg-zinc-900/60 px-2.5 py-2"
            >
              <Film
                size={15}
                strokeWidth={1.75}
                aria-hidden="true"
                className="shrink-0 text-accent-bright"
              />
              <div className="min-w-0 flex-1">
                <p className="text-[11px] text-zinc-400">
                  {SOURCE_LABELS[candidate.source]}
                </p>
                <p className="truncate text-[12px] text-zinc-100" title={label}>
                  {label}
                </p>
              </div>
              <Button className="shrink-0" onClick={() => onSelect(candidate)}>
                Review
              </Button>
            </li>
          );
        })}
      </ul>
    </aside>
  );
}
