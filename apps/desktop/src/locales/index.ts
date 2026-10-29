import en from "./en.json";
import zhCN from "./zh-CN.json";

export function getMessages(language: string): typeof en {
  return language.toLowerCase().startsWith("zh") ? zhCN : en;
}
