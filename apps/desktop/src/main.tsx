import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import * as backend from "@/api/backend";
import App from "./App";
import { CapturePage } from "./capture/CapturePage";
import { storedBoardSize } from "./capture/geometry";
import { FloatingBoard } from "./floating/FloatingBoard";
import { getMessages } from "./locales";
import "./styles/app.css";
import "./styles/frame.css";
import "./styles/side.css";
import "./styles/batch.css";
import "./styles/play.css";
import "./styles/coach.css";
import "./styles/online.css";
import "./styles/remote.css";
import "./styles/research.css";
import "./styles/analysis-tools.css";
import "./styles/layout.css";
import "./styles/floating.css";
import "./styles/acceleration.css";
import "./styles/webboard.css";
import "./styles/comparison.css";
import "./styles/capture.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("Application root element is missing");
}

document.title = getMessages(navigator.language).app.title;
document.documentElement.lang = navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";

// The shell opens 浮动主棋盘 and 浮动小棋盘 on the same page with `?window=float-main|float-sub`,
// and 抓取死活题's window with `?window=capture&session=<id>`.
const params = new URLSearchParams(window.location.search);
const floating = params.get("window");
const kind = floating === "float-main" ? "Main" : floating === "float-sub" ? "Sub" : null;
const sessionParam = params.get("session");
const session = floating === "capture" && sessionParam !== null && sessionParam !== "" ? Number(sessionParam) : NaN;

function windowStorage(): Storage | null {
  try {
    return localStorage;
  } catch {
    return null;
  }
}

const page = Number.isInteger(session) ? (
  <CapturePage backend={backend} session={session} labels={getMessages(navigator.language).capture} preferredSize={storedBoardSize(windowStorage())} />
) : kind ? (
  <FloatingBoard kind={kind} backend={backend} />
) : (
  <App />
);
createRoot(root).render(<StrictMode>{page}</StrictMode>);
