import type { SniffCandidate } from "../../types";
import { MediaPicker, type MediaSelection } from "./MediaPicker";

interface MediaCandidatePopupProps {
  readonly pageUrl: string;
  readonly candidates: readonly SniffCandidate[];
  readonly onSelect: (selection: MediaSelection) => void;
  readonly onDismiss: () => void;
}

/** The inspector entry point. The picker owns grouping and quality menus. */
export function MediaCandidatePopup({
  pageUrl,
  candidates,
  onSelect,
  onDismiss,
}: MediaCandidatePopupProps) {
  return (
    <MediaPicker
      pageUrl={pageUrl}
      candidates={candidates}
      onSelect={onSelect}
      onDismiss={onDismiss}
    />
  );
}
