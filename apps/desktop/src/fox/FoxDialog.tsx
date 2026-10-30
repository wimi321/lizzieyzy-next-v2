import { type FormEvent, useEffect, useState } from "react";
import type { Backend, FoxAccount, FoxGame, FoxQuery } from "@/api/backend";
import { Icon } from "../toolbar/icons";
import { type FoxErrorLabels, type FoxResultLabels, errorText, resultText, shortTime } from "./format";

export interface FoxLabels {
  title: string;
  nickname: string;
  uid: string;
  placeholderNickname: string;
  placeholderUid: string;
  search: string;
  recent: string;
  clearRecent: string;
  intro: string;
  loading: string;
  account: string;
  more: string;
  loadingMore: string;
  noGames: string;
  noMore: string;
  time: string;
  black: string;
  white: string;
  result: string;
  moves: string;
  opening: string;
  closeAfterOpen: string;
  close: string;
  results: FoxResultLabels;
  errors: FoxErrorLabels;
}

type FoxBackend = Pick<Backend, "foxSearch" | "foxNextPage" | "foxOpenGame" | "foxClearHistory">;

export interface FoxDialogProps {
  backend: FoxBackend;
  labels: FoxLabels;
  /** Recent accounts from the settings, newest first. */
  history: readonly FoxAccount[];
  onHistoryChanged(): void;
  onClose(): void;
}

type Status =
  | { kind: "idle" }
  | { kind: "searching" }
  | { kind: "error"; message: string }
  | { kind: "ready"; account: FoxAccount; games: FoxGame[]; cursor: string | null; loadingMore: boolean };

/** LizzieYzy Next's Fox kifu window: search by nickname or UID, list recent games, open one. */
export function FoxDialog({ backend, labels, history, onHistoryChanged, onClose }: FoxDialogProps) {
  const [mode, setMode] = useState<FoxQuery["type"]>("Nickname");
  const [text, setText] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const [opening, setOpening] = useState<string | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);
  const [closeAfterOpen, setCloseAfterOpen] = useState(true);
  const busy = status.kind === "searching" || opening !== null;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const search = (query: FoxQuery) => {
    setStatus({ kind: "searching" });
    setOpenError(null);
    backend
      .foxSearch(query)
      .then((result) => {
        setStatus({ kind: "ready", account: result.account, games: result.page.games, cursor: result.page.next_cursor, loadingMore: false });
        onHistoryChanged();
      })
      .catch((error: unknown) => setStatus({ kind: "error", message: errorText(error, labels.errors) }));
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const value = text.trim();
    if (!value || busy) return;
    search({ type: mode, value });
  };

  const loadMore = () => {
    if (status.kind !== "ready" || !status.cursor || status.loadingMore) return;
    const { account, cursor } = status;
    setStatus({ ...status, loadingMore: true });
    backend
      .foxNextPage(account.uid, cursor)
      .then((page) =>
        setStatus((current) =>
          current.kind === "ready" && current.account.uid === account.uid
            ? { ...current, games: [...current.games, ...page.games], cursor: page.next_cursor, loadingMore: false }
            : current,
        ),
      )
      .catch((error: unknown) => {
        setStatus((current) => (current.kind === "ready" ? { ...current, loadingMore: false } : current));
        setOpenError(errorText(error, labels.errors));
      });
  };

  const open = (game: FoxGame) => {
    if (opening) return;
    setOpening(game.id);
    setOpenError(null);
    backend
      .foxOpenGame(game.id)
      .then(() => {
        setOpening(null);
        if (closeAfterOpen) onClose();
      })
      .catch((error: unknown) => {
        setOpening(null);
        setOpenError(errorText(error, labels.errors));
      });
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog fox-dialog" role="dialog" aria-modal="true" aria-labelledby="fox-title" data-testid="fox-dialog">
        <header className="dialog-header">
          <h2 id="fox-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} data-testid="fox-close" onClick={onClose}>
            ×
          </button>
        </header>
        <form className="fox-search" onSubmit={submit}>
          <div className="segmented small" role="radiogroup">
            {(["Nickname", "Uid"] as const).map((value) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={mode === value}
                data-testid={`fox-mode-${value.toLowerCase()}`}
                onClick={() => setMode(value)}
              >
                {value === "Nickname" ? labels.nickname : labels.uid}
              </button>
            ))}
          </div>
          <input
            className="text-input"
            data-testid="fox-input"
            value={text}
            inputMode={mode === "Uid" ? "numeric" : undefined}
            placeholder={mode === "Uid" ? labels.placeholderUid : labels.placeholderNickname}
            onChange={(event) => setText(event.target.value)}
            autoFocus
          />
          <button type="submit" className="primary-button" data-testid="fox-search" disabled={busy || !text.trim()}>
            {labels.search}
          </button>
        </form>
        {history.length > 0 ? (
          <div className="fox-recent">
            <span className="fox-recent-label">{labels.recent}</span>
            {history.map((account) => (
              <button
                key={account.uid}
                type="button"
                className="tag"
                data-testid="fox-recent"
                disabled={busy}
                title={`UID ${account.uid}`}
                onClick={() => {
                  setMode("Uid");
                  setText(account.uid);
                  search({ type: "Uid", value: account.uid });
                }}
              >
                {account.nickname}
              </button>
            ))}
            <button
              type="button"
              className="link-button"
              data-testid="fox-clear-recent"
              disabled={busy}
              onClick={() => void backend.foxClearHistory().then(onHistoryChanged).catch(() => {})}
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
            <div className="fox-state" data-testid="fox-loading">
              <span className="spinner" aria-hidden="true" />
              <p>{labels.loading}</p>
            </div>
          ) : null}
          {status.kind === "error" ? (
            <p className="fox-error" role="alert" data-testid="fox-error">
              {status.message}
            </p>
          ) : null}
          {status.kind === "ready" ? (
            <>
              <p className="fox-account" data-testid="fox-account">
                {labels.account.replace("{name}", status.account.nickname).replace("{uid}", status.account.uid)}
              </p>
              {status.games.length > 0 ? (
                <div className="fox-table" role="table">
                  <div className="fox-row fox-head" role="row">
                    <span role="columnheader">{labels.time}</span>
                    <span role="columnheader">{labels.black}</span>
                    <span role="columnheader">{labels.white}</span>
                    <span role="columnheader">{labels.result}</span>
                    <span role="columnheader">{labels.moves}</span>
                  </div>
                  {status.games.map((game) => (
                    <button
                      key={game.id}
                      type="button"
                      role="row"
                      className="fox-row"
                      data-testid="fox-game"
                      disabled={opening !== null}
                      aria-busy={opening === game.id}
                      onClick={() => open(game)}
                    >
                      <span role="cell" className="fox-time">{opening === game.id ? labels.opening : shortTime(game.start_time)}</span>
                      <Player player={game.black} own={game.black.uid === status.account.uid} />
                      <Player player={game.white} own={game.white.uid === status.account.uid} />
                      <span role="cell" className="fox-result">
                        <span className={`fox-outcome ${outcome(game, status.account.uid)}`} aria-hidden="true" />
                        {resultText(game.result, labels.results)}
                      </span>
                      <span role="cell" className="fox-moves">{game.move_count}</span>
                    </button>
                  ))}
                </div>
              ) : (
                <div className="fox-state">
                  <p>{labels.noGames}</p>
                </div>
              )}
              {status.games.length > 0 ? (
                status.cursor ? (
                  <button type="button" className="secondary-button fox-more" data-testid="fox-more" disabled={status.loadingMore} onClick={loadMore}>
                    {status.loadingMore ? labels.loadingMore : labels.more}
                  </button>
                ) : (
                  <p className="panel-empty fox-end">{labels.noMore}</p>
                )
              ) : null}
            </>
          ) : null}
          {openError ? (
            <p className="fox-error" role="alert" data-testid="fox-open-error">
              {openError}
            </p>
          ) : null}
        </div>
        <footer className="fox-footer">
          <label className="checkbox">
            <input type="checkbox" checked={closeAfterOpen} onChange={(event) => setCloseAfterOpen(event.target.checked)} />
            {labels.closeAfterOpen}
          </label>
        </footer>
      </section>
    </div>
  );
}

/** Whether the searched account won, so lost games stand out when picking one to review. */
function outcome(game: FoxGame, uid: string): "win" | "loss" | "none" {
  const winner = game.result.winner;
  if (!winner) return "none";
  const own = game.black.uid === uid ? "Black" : game.white.uid === uid ? "White" : null;
  if (!own) return "none";
  return winner === own ? "win" : "loss";
}

function Player({ player, own }: { player: FoxGame["black"]; own: boolean }) {
  return (
    <span role="cell" className={own ? "fox-player own" : "fox-player"}>
      <span className="fox-name">{player.nickname}</span>
      <span className="fox-rank">{player.rank}</span>
    </span>
  );
}
