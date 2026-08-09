import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";

import App from "./App";
import { desktopApi } from "./api/desktop";
import { TrayPanel } from "./components/TrayPanel";
import "./app.css";

const isTrayPanel =
  (desktopApi.isDesktopRuntime() && getCurrentWindow().label === "tray-panel") ||
  (import.meta.env.DEV &&
    new URLSearchParams(window.location.search).has("tray-panel-preview"));

if (isTrayPanel) {
  document.documentElement.classList.add("tray-panel-document");
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {isTrayPanel ? <TrayPanel /> : <App />}
  </StrictMode>,
);
