import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "./index.css";
import App from "./App";
import { applyTheme, readTheme } from "./lib/preferences";

applyTheme(readTheme());

const rootElement = document.getElementById("root");
if (rootElement === null) {
  throw new Error("Root element is missing from index.html");
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
