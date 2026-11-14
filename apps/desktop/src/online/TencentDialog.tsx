import { type FormEvent, useEffect, useRef, useState } from "react";
import type { Backend } from "@/api/backend";
import type { TencentGame } from "../api/tencent";
import { resultText } from "../fox/format";
import type en from "../locales/en.json";
import { Icon } from "../toolbar/icons";
import { isQueried, playerText, rankText, startTime, tencentError } from "./format";

type Labels = typeof en.online.tencent;
type TencentBackend = Pick<Backend, "tencentSearch" | "tencentNextPage" | "tencentOpenGame" | "tencentClearHistory">;

export interface TencentDialogProps {
  /** Hidden rather than unmounted, so a minimized window keeps its list. */
  open: boolean;
  backend: TencentBackend;
  labels: Labels;
  results: typeof en.fox.results;
  /** Recent queries from the settings, newest first. */
  history: readonly string[];
  onHistoryChanged(): void;
  onClose(): void;
}

type AfterOpen = "minimize" | "close" | "stay";
const AFTER_OPEN: readonly AfterOpen[] = ["minimize", "close", "stay"];

type Status =
  | { kind: "idle" }
  | { kind: "searching"; query: string }
  | { kind: "error"; message: string }
  | { kind: "ready"; query: string; games: TencentGame[]; cursor: string | null; loadingMore: boolean };

/** Java's 腾讯棋谱 window: a nickname or UID, the account's recent public games, open one. */
export function TencentDialog({ open: visible, backend, labels, results, history, onHistoryChanged, onClose }: TencentDialogProps) {
  const [text, setText] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const [opening, setOpening] = useState<string | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);
  const [afterOpen, setAfterOpen] = useState<AfterOpen>("minimize");
  const input = useRef<HTMLInputElement>(null);
  const busy = status.kind === "searching" || opening !== null;

  useEffect(() => {
    if (visible) input.current?.focus();
  }, [visible]);

  useEffect(() => {
    if (!visible) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [visible, onClose]);

  const search = (query: string) => {
    if (!backend.tencentSearch) return;
    setStatus({ kind: "searching", query });
    setOpenError(null);
    backend
      .tencentSearch(query)
      .then((page) => {
        setStatus({ kind: "ready", query, games: page.games ?? [], cursor: page.next_cursor ?? null, loadingMore: false });
        onHistoryChanged();
      })
      .catch((error: unknown) => setStatus({ kind: "error", message: tencentError(error, labels.errors) }));
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const value = text.trim();
    if (value && !busy) search(value);
  };

  const loadMore = () => {
    if (status.kind !== "ready" || !status.cursor || status.loadingMore || !backend.tencentNextPage) return;
    const { query, cursor } = status;
    setStatus({ ...status, loadingMore: true });
    backend
      .tencentNextPage(query, cursor)
      .then((page) =>
        setStatus((current) =>
          current.kind === "ready" && current.query === query
            ? { ...current, games: [...current.games, ...(page.games ?? [])], cursor: page.next_cursor ?? null, loadingMore: false }
            : current,
        ),
      )
      .catch((error: unknown) => {
        setStatus((current) => (current.kind === "ready" ? { ...current, loadingMore: false } : current));
        setOpenError(tencentError(error, labels.errors));
      });
  };

  const open = (game: TencentGame) => {
    if (opening || !game.id || !backend.tencentOpenGame) return;
    setOpening(game.id);
    setOpenError(null);
    backend
      .tencentOpenGame(game.id)
      .then(() => {
        setOpening(null);
        if (afterOpen === "stay") return;
        if (afterOpen === "close") setStatus({ kind: "idle" });
        onClose();
      })
      .catch((error: unknown) => {
        setOpening(null);
        setOpenError(tencentError(error, labels.errors));
      });
  };

  const rank = (game: TencentGame, side: "black" | "white") => rankText(game[side]?.rank, labels);

  return (
    <div className="dialog-backdrop" role="presentation" hidden={!visible}>
      <section className="dialog fox-dialog tencent-dialog" role="dialog" aria-modal="true" aria-labelledby="tencent-title" data-testid="tencent-dialog">
        <header className="dialog-header">
          <h2 id="tencent-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} data-testid="tencent-close" onClick={onClose}>
            ×
          </button>
        </header>
        <form className="fox-search" onSubmit={submit}>
          <label className="tencent-label" htmlFor="tencent-input">
            {labels.query}
          </label>
          <input
            ref={input}
            id="tencent-input"
            className="text-input"
            data-testid="tencent-input"
            value={text}
            placeholder={labels.placeholder}
            onChange={(event) => setText(event.target.value)}
          />
          <button type="submit" className="primary-button" data-testid="tencent-search" disabled={busy || !text.trim()}>
            {labels.search}
          </button>
        </form>
        {history.length > 0 ? (
          <div className="fox-recent">
            <span className="fox-recent-label">{labels.recent}</span>
            {history.map((query) => (
              <button
                key={query}
                type="button"
                className="tag"
                data-testid="tencent-recent"
                disabled={busy}
                onClick={() => {
                  setText(query);
                  search(query);
                }}
              >
                {query}
              </button>
            ))}
            <button
              type="button"
              className="link-button"
              data-testid="tencent-clear-recent"
              disabled={busy}
              onClick={() => void backend.tencentClearHistory?.().then(onHistoryChanged).catch(() => {})}
            >
              {labels.clearRecent}
            </button>
          </div>
        ) : null}
        <div className="fox-body" aria-live="polite">
          {status.kind === "idle" ? (
            <div className="fox-state">
              <span className="fox-state-icon" aria-hidden="true">
                <Icon name="fox" />
              </span>
              <p>{labels.intro}</p>
            </div>
          ) : null}
          {status.kind === "searching" ? (
            <div className="fox-state" data-testid="tencent-loading">
              <span className="spinner" aria-hidden="true" />
              <p>{labels.loading.replace("{query}", status.query)}</p>
            </div>
          ) : null}
          {status.kind === "error" ? (
            <p className="fox-error" role="alert" data-testid="tencent-error">
              {status.message}
            </p>
          ) : null}
          {status.kind === "ready" ? (
            <>
              <p className="fox-account" data-testid="tencent-current">
                {labels.current.replace("{query}", status.query)}
                <span className="tencent-open-hint">{labels.openHint}</span>
              </p>
              {status.games.length > 0 ? (
                <div className="fox-table tencent-table" role="table">
                  <div className="fox-row fox-head" role="row">
                    {[labels.index, labels.time, labels.black, labels.rank, labels.white, labels.rank, labels.result, labels.moves, labels.open].map((name, column) => (
                      <span key={column} role="columnheader">
                        {name}
                      </span>
                    ))}
                  </div>
                  {status.games.map((game, index) => {
                    const won = (game.result?.winner === "Black" && isQueried(game.black, status.query)) || (game.result?.winner === "White" && isQueried(game.white, status.query));
                    return (
                      <div
                        key={game.id ?? index}
                        role="row"
                        className={won ? "fox-row tencent-row won" : "fox-row tencent-row"}
                        data-testid="tencent-game"
                        aria-busy={opening === game.id}
                        onDoubleClick={() => open(game)}
                      >
                        <span role="cell" className="fox-moves">{index + 1}</span>
                        <span role="cell" className="fox-time">{opening === game.id ? labels.opening : startTime(game.start_time)}</span>
                        <span role="cell" className="fox-name" title={playerText(game.black)}>{playerText(game.black)}</span>
                        <span role="cell" className="fox-rank">{rank(game, "black")}</span>
                        <span role="cell" className="fox-name" title={playerText(game.white)}>{playerText(game.white)}</span>
                        <span role="cell" className="fox-rank">{rank(game, "white")}</span>
                        <span role="cell" className="fox-result">{game.result ? resultText(game.result, results) : ""}</span>
                        <span role="cell" className="fox-moves">{game.move_count ?? 0}</span>
                        <span role="cell">
                          <button type="button" className="secondary-button small" data-testid="tencent-open" disabled={opening !== null} onClick={() => open(game)}>
                            {labels.open}
                          </button>
                        </span>
                      </div>
                    );
                  })}
                </div>
              ) : (
                <div className="fox-state">
                  <p>{labels.noGames}</p>
                </div>
              )}
              {status.games.length > 0 ? (
                status.cursor ? (
                  <button type="button" className="secondary-button fox-more" data-testid="tencent-more" disabled={status.loadingMore} onClick={loadMore}>
                    {status.loadingMore ? labels.loadingMore : labels.more}
                  </button>
                ) : (
                  <p className="panel-empty fox-end">{labels.noMore}</p>
                )
              ) : null}
            </>
          ) : null}
          {openError ? (
            <p className="fox-error" role="alert" data-testid="tencent-open-error">
              {openError}
            </p>
          ) : null}
        </div>
        <footer className="fox-footer">
          <span className="fox-after-label" id="tencent-after-open">
            {labels.afterOpen}
          </span>
          <div className="segmented small" role="radiogroup" aria-labelledby="tencent-after-open">
            {AFTER_OPEN.map((value) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={afterOpen === value}
                data-testid={`tencent-after-${value}`}
                onClick={() => setAfterOpen(value)}
              >
                {value === "minimize" ? labels.afterOpenMinimize : value === "close" ? labels.afterOpenClose : labels.afterOpenStay}
              </button>
            ))}
          </div>
        </footer>
      </section>
    </div>
  );
}
