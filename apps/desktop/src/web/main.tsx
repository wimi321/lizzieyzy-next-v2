import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { messagesFor } from "./prefs";
import { WebPage } from "./WebPage";
import "../styles/app.css";
import "../styles/web-page.css";

// The browser page that LizzieYzy's 网页旁观 serves on the local network (P3-33). It talks only to
// the host's WebSocket: no Tauri API, no desktop backend.
const root = document.getElementById("root");
if (!root) throw new Error("Web board root element is missing");

const messages = messagesFor(navigator.languages?.length ? navigator.languages : [navigator.language]);
document.title = messages.webBoard.page.title;
document.documentElement.lang = messages === messagesFor(["en"]) ? "en" : navigator.language;

/** A browser may refuse storage (private windows, blocked cookies); the page still works. */
function localStore(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}
const storage = localStore();

createRoot(root).render(
  <StrictMode>
    <WebPage messages={messages} location={window.location} storage={storage} />
  </StrictMode>,
);
