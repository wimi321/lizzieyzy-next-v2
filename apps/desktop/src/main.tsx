import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import * as backend from "@/api/backend";
import App from "./App";
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

const root = document.getElementById("root");
if (!root) {
  throw new Error("Application root element is missing");
}

document.title = getMessages(navigator.language).app.title;
document.documentElement.lang = navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";

// The shell opens 浮动主棋盘 and 浮动小棋盘 on the same page with `?window=float-main|float-sub`.
const floating = new URLSearchParams(window.location.search).get("window");
const kind = floating === "float-main" ? "Main" : floating === "float-sub" ? "Sub" : null;

createRoot(root).render(<StrictMode>{kind ? <FloatingBoard kind={kind} backend={backend} /> : <App />}</StrictMode>);
