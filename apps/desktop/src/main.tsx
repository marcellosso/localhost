import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { guardContextMenu } from "./webview";
import "./styles/tokens.css";
import "./styles/app.css";

guardContextMenu();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
