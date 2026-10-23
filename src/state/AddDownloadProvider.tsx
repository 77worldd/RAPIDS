import {
  createContext,
  useCallback,
  useContext,
  useMemo,
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

  const openAdd = useCallback((url?: string) => {
    setHandoff(null);
    setInitialUrl(url !== undefined && url.trim() !== "" ? url.trim() : null);
    setSession((current) => current + 1);
    setIsOpen(true);
  }, []);

  const openHandoff = useCallback((request: DeepLinkRequest) => {
    setHandoff(request);
    setInitialUrl(request.url);
    setSession((current) => current + 1);
    setIsOpen(true);
  }, []);

  const closeAdd = useCallback(() => {
    setIsOpen(false);
    setInitialUrl(null);
    setHandoff(null);
  }, []);

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
