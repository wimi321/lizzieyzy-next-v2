import en from "./en.json";
import ja from "./ja.json";
import ko from "./ko.json";
import th from "./th.json";
import zhCN from "./zh-CN.json";
import zhTW from "./zh-TW.json";

export function getMessages(language: string): typeof en {
  const [locale, variant = ""] = language.trim().toLowerCase().replaceAll("_", "-").split("-");
  switch (locale) {
    case "zh":
      return ["tw", "hk", "hant"].includes(variant) ? zhTW : zhCN;
    case "ja":
      return ja;
    case "ko":
      return ko;
    case "th":
      return th;
    default:
      return en;
  }
}
