import { AppShell } from "./components/layout/AppShell";
import { StoreProvider } from "./state/StoreProvider";

export default function App() {
  return (
    <StoreProvider>
      <AppShell />
    </StoreProvider>
  );
}
