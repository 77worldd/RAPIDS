import { AppShell } from "./components/layout/AppShell";
import { DeepLinkBridge } from "./components/DeepLinkBridge";
import { AddDownloadProvider } from "./state/AddDownloadProvider";
import { StoreProvider } from "./state/StoreProvider";

export default function App() {
  return (
    <StoreProvider>
      <AddDownloadProvider>
        <DeepLinkBridge />
        <AppShell />
      </AddDownloadProvider>
    </StoreProvider>
  );
}
