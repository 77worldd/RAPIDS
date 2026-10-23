import { AppShell } from "./components/layout/AppShell";
import { AddDownloadProvider } from "./state/AddDownloadProvider";
import { StoreProvider } from "./state/StoreProvider";

export default function App() {
  return (
    <StoreProvider>
      <AddDownloadProvider>
        <AppShell />
      </AddDownloadProvider>
    </StoreProvider>
  );
}
