import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { AddDownloadDialog } from "../components/tasks/AddDownloadDialog";

interface AddDownloadContextValue {
  readonly isOpen: boolean;
  readonly openAdd: (url?: string) => void;
  readonly closeAdd: () => void;
}

const AddDownloadContext = createContext<AddDownloadContextValue | null>(null);

export function AddDownloadProvider({ children }: { readonly children: ReactNode }) {
  const [isOpen, setIsOpen] = useState(false);
  const [initialUrl, setInitialUrl] = useState<string | null>(null);

  const openAdd = useCallback((url?: string) => {
    setInitialUrl(url !== undefined && url.trim() !== "" ? url.trim() : null);
    setIsOpen(true);
  }, []);

  const closeAdd = useCallback(() => {
    setIsOpen(false);
    setInitialUrl(null);
  }, []);

  const value = useMemo<AddDownloadContextValue>(
    () => ({ isOpen, openAdd, closeAdd }),
    [closeAdd, isOpen, openAdd],
  );

  return (
    <AddDownloadContext.Provider value={value}>
      {children}
      {isOpen && (
        <AddDownloadDialog
          onClose={closeAdd}
          {...(initialUrl !== null ? { initialUrl } : {})}
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
