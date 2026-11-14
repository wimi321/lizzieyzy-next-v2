import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { getMessages } from "./locales";
import "./styles/app.css";
import "./styles/frame.css";
import "./styles/side.css";
import "./styles/batch.css";
import "./styles/play.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("Application root element is missing");
}

document.title = getMessages(navigator.language).app.title;
document.documentElement.lang = navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";

createRoot(root).render(<StrictMode><App /></StrictMode>);
