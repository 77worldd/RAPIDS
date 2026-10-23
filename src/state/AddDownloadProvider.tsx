import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AddDownloadDialog } from "../components/tasks/AddDownloadDialog";
import type { DeepLinkRequest } from "../lib/deeplink";

interface AddDownloadContextValue {
  readonly isOpen: boolean;
  readonly openAdd: (url?: string) => void;
  readonly openHandoff: (request: DeepLinkRequest) => void;
  readonly closeAdd: () => void;
}

const AddDownloadContext = createContext<AddDownloadContextValue | null>(null);

export function AddDownloadProvider({
  children,
}: {
  readonly children: ReactNode;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const [initialUrl, setInitialUrl] = useState<string | null>(null);
  const [handoff, setHandoff] = useState<DeepLinkRequest | null>(null);
  const [session, setSession] = useState(0);
  // A sheet shows one handoff at a time; keep later links in arrival order.
  const queuedHandoffs = useRef<DeepLinkRequest[]>([]);
  const sheetOpen = useRef(false);

  const showHandoff = useCallback((request: DeepLinkRequest) => {
    setHandoff(request);
    setInitialUrl(request.url);
    setSession((current) => current + 1);
    setIsOpen(true);
    sheetOpen.current = true;
  }, []);

  const openAdd = useCallback((url?: string) => {
    queuedHandoffs.current = [];
    setHandoff(null);
    setInitialUrl(url !== undefined && url.trim() !== "" ? url.trim() : null);
    setSession((current) => current + 1);
    setIsOpen(true);
    sheetOpen.current = true;
  }, []);

  const openHandoff = useCallback(
    (request: DeepLinkRequest) => {
      if (sheetOpen.current) {
        queuedHandoffs.current.push(request);
        return;
      }
      showHandoff(request);
    },
    [showHandoff],
  );

  const closeAdd = useCallback(() => {
    const next = queuedHandoffs.current.shift();
    if (next !== undefined) {
      showHandoff(next);
      return;
    }
    setIsOpen(false);
    setInitialUrl(null);
    setHandoff(null);
    sheetOpen.current = false;
  }, [showHandoff]);

  const value = useMemo<AddDownloadContextValue>(
    () => ({ isOpen, openAdd, openHandoff, closeAdd }),
    [closeAdd, isOpen, openAdd, openHandoff],
  );

  return (
    <AddDownloadContext.Provider value={value}>
      {children}
      {isOpen && (
        <AddDownloadDialog
          key={session}
          onClose={closeAdd}
          {...(initialUrl !== null ? { initialUrl } : {})}
          {...(handoff !== null ? { handoff } : {})}
        />
      )}
    </AddDownloadContext.Provider>
  );
}

export function useAddDownload(): AddDownloadContextValue {
  const value = useContext(AddDownloadContext);
  if (value === null) {
    throw new Error("useAddDownload must be used inside AddDownloadProvider");
  }
  return value;
}
