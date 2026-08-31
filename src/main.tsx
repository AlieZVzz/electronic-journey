import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { TrayMenu } from "./components/TrayMenu";
import "./app.css";

const rootView = new URLSearchParams(window.location.search).get("window");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {rootView === "tray-menu" ? <TrayMenu /> : <App />}
  </StrictMode>,
);
