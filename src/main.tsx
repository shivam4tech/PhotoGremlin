import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/theme.css";
import { applyTheme, readStoredTheme } from "./lib/theme";

// Apply the stored theme before first paint so the window never flashes
// the wrong appearance on start.
applyTheme(readStoredTheme());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
