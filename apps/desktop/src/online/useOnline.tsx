import { useCallback, useState } from "react";
import type { Backend, FoxAccount, Settings } from "@/api/backend";
import { FoxDialog } from "../fox/FoxDialog";
import type en from "../locales/en.json";
import { OpenUrlDialog } from "./OpenUrlDialog";
import { TencentDialog } from "./TencentDialog";

type OnlineBackend = Pick<
  Backend,
  "settings" | "foxSearch" | "foxNextPage" | "foxOpenGame" | "foxClearHistory" | "tencentSearch" | "tencentNextPage" | "tencentOpenGame" | "tencentClearHistory" | "openUrl"
>;

function report(error: unknown) {
  console.error("Online kifu command failed", error);
}

/** A kifu window is mounted on first use and then only hidden, so a minimized one keeps its list. */
interface KifuWindow {
  used: boolean;
  open: boolean;
}

const CLOSED: KifuWindow = { used: false, open: false };

/**
 * 野狐棋谱, 腾讯棋谱 and 文件 → 打开在线链接 (Q). Rust searches, downloads and keeps the query
 * history; this opens the windows and reads the history back from the settings.
 */
export function useOnline(backend: OnlineBackend, messages: Pick<typeof en, "fox" | "online">) {
  const [fox, setFox] = useState(CLOSED);
  const [tencent, setTencent] = useState(CLOSED);
  const [urlOpen, setUrlOpen] = useState(false);
  const [foxHistory, setFoxHistory] = useState<FoxAccount[]>([]);
  const [tencentHistory, setTencentHistory] = useState<string[]>([]);

  const load = useCallback((settings: Settings) => {
    setFoxHistory(settings.fox_history);
    setTencentHistory(settings.tencent_history ?? []);
  }, []);
  // The backend records searches in the settings; read them back rather than guess.
  const refresh = () => void backend.settings().then(load).catch(report);
  const openFox = () => setFox({ used: true, open: true });
  const openTencent = () => setTencent({ used: true, open: true });
  const closeFox = useCallback(() => setFox((window) => ({ ...window, open: false })), []);
  const closeTencent = useCallback(() => setTencent((window) => ({ ...window, open: false })), []);
  const closeUrl = useCallback(() => setUrlOpen(false), []);
  const openUrl = () => setUrlOpen(true);

  const dialogs = (
    <>
      {fox.used ? <FoxDialog open={fox.open} backend={backend} labels={messages.fox} history={foxHistory} onHistoryChanged={refresh} onClose={closeFox} /> : null}
      {tencent.used ? (
        <TencentDialog
          open={tencent.open}
          backend={backend}
          labels={messages.online.tencent}
          results={messages.fox.results}
          history={tencentHistory}
          onHistoryChanged={refresh}
          onClose={closeTencent}
        />
      ) : null}
      {urlOpen ? <OpenUrlDialog backend={backend} labels={messages.online.url} onClose={closeUrl} /> : null}
    </>
  );

  return { load, openFox, openTencent, openUrl, dialogs, dialogOpen: fox.open || tencent.open || urlOpen };
}
