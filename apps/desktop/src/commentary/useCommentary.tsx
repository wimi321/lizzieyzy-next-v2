import { useCallback, useEffect, useRef, useState } from "react";
import type { Backend, BoardView, Settings } from "@/api/backend";
import type { CommentaryKeyStatus, CommentaryMode, CommentarySettings, CommentaryUpdated } from "../api/commentary";
import { ToolButton } from "../frame/ToolButton";
import type en from "../locales/en.json";
import type { MenuBindings } from "../menu/bindings";
import { type CommentaryEntry, type CommentaryModeChoice, CommentaryWindow } from "./CommentaryWindow";
import { CommentarySetup } from "./CommentarySetup";
import { commentaryError, isWorking } from "./format";

type CommentaryBackend = Pick<
  Backend,
  | "startCommentary"
  | "stopCommentary"
  | "commentaryModels"
  | "setCommentaryKey"
  | "clearCommentaryKey"
  | "commentaryKeyStatus"
  | "setCommentarySettings"
  | "subscribeCommentaryUpdated"
  | "unsubscribe"
>;

/** What Rust saves when the settings have no commentary yet (`CommentarySettings::default`). */
const DEFAULTS: Required<CommentarySettings> = {
  base_url: "",
  model: "",
  remember_key: false,
  rank: "5k",
  style: "Balanced",
  terminology: "Medium",
  detail: "Standard",
  variations: "Moderate",
  write_to_comment: true,
};

function report(error: unknown) {
  console.error("AI commentary command failed", error);
}

export interface CommentaryPlace {
  board: BoardView | null;
  jump(node: number): void;
}

/**
 * P3-19: AI 解说. Rust selects the positions, asks the service, checks the reply and writes it
 * into the game; this keeps the window, the conversation and the service settings.
 */
export function useCommentary(backend: CommentaryBackend, labels: typeof en.commentary, place: CommentaryPlace) {
  const [settings, setSettings] = useState<Required<CommentarySettings>>(DEFAULTS);
  const [keyStatus, setKeyStatus] = useState<CommentaryKeyStatus | null>(null);
  const [open, setOpen] = useState(false);
  const [setupOpen, setSetupOpen] = useState(false);
  const [mode, setMode] = useState<CommentaryModeChoice>("next");
  const [range, setRange] = useState({ from: "", to: "" });
  const [entries, setEntries] = useState<CommentaryEntry[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const conversation = useRef<CommentaryEntry[]>([]);
  // The question of the request just sent: null for a fresh commentary, undefined once taken.
  const outgoing = useRef<string | null | undefined>(undefined);
  // Rust's events win: a reply only counts if no event came after the request.
  const events = useRef(0);

  const apply = useCallback((view: CommentaryUpdated) => {
    const id = view.request_id ?? 0;
    const list = conversation.current;
    const at = list.findIndex((entry) => entry.id === id);
    let next: CommentaryEntry[];
    if (at >= 0) next = list.map((entry, index) => (index === at ? { ...entry, view } : entry));
    else if (list.length > 0 && id < (list[list.length - 1]?.id ?? 0)) return;
    else {
      const question = outgoing.current ?? null;
      outgoing.current = undefined;
      next = question === null ? [{ id, question, view }] : [...list, { id, question, view }];
    }
    conversation.current = next;
    setEntries(next);
  }, []);

  useEffect(() => {
    if (!backend.subscribeCommentaryUpdated) return;
    let active = true;
    const id = backend.subscribeCommentaryUpdated((view) => {
      if (!active) return;
      events.current += 1;
      apply(view);
    });
    return () => {
      active = false;
      void id.then((value) => backend.unsubscribe(value)).catch(() => undefined);
    };
  }, [backend, apply]);

  const load = useCallback((saved: Settings) => setSettings({ ...DEFAULTS, ...saved.commentary }), []);
  const describe = useCallback((error: unknown) => commentaryError(error, labels), [labels]);
  const readKey = () => {
    if (!backend.commentaryKeyStatus) return Promise.resolve();
    return backend.commentaryKeyStatus().then(setKeyStatus);
  };

  const last = entries[entries.length - 1];
  const busy = isWorking(last?.view);
  const configured = settings.base_url !== "" && settings.model !== "" && keyStatus?.has_key === true;
  const current = place.board ? place.board.move_number : null;

  const show = () => {
    setOpen(true);
    setFailure(null);
    if (range.from === "" && range.to === "") {
      const to = Math.max(1, current ?? 1);
      setRange({ from: String(Math.max(1, to - 9)), to: String(to) });
    }
    void readKey().catch(report);
  };
  const close = useCallback(() => {
    setOpen(false);
    if (isWorking(conversation.current[conversation.current.length - 1]?.view)) void backend.stopCommentary?.().catch(report);
  }, [backend]);

  const send = (question: string | null) => {
    if (!backend.startCommentary) return;
    let chosen: CommentaryMode = { type: mode === "next" ? "NextMove" : "WholeGame" };
    if (mode === "range") {
      const from = Number(range.from);
      const to = Number(range.to);
      if (!Number.isInteger(from) || !Number.isInteger(to) || from < 1 || from > to) return setFailure(labels.errors.InvalidRange);
      chosen = { type: "Range", data: { from, to } };
    }
    setFailure(null);
    outgoing.current = question;
    const sent = events.current;
    backend
      .startCommentary({ mode: chosen, follow_up: question })
      .then((view) => {
        if (events.current === sent) apply(view);
      })
      .catch((error: unknown) => {
        outgoing.current = undefined;
        setFailure(describe(error));
      });
  };
  const stop = () => {
    const sent = events.current;
    backend
      .stopCommentary?.()
      .then((view) => {
        if (events.current === sent) apply(view);
      })
      .catch(report);
  };
  const writeToComment = (on: boolean) => {
    const next = { ...settings, write_to_comment: on };
    setSettings(next);
    backend
      .setCommentarySettings?.(next)
      .then((saved) => setSettings({ ...DEFAULTS, ...saved }))
      .catch((error: unknown) => setFailure(describe(error)));
  };

  // The service and its key are saved before the models are asked for: Rust reads them from there.
  const connect = async (form: CommentarySettings, key: string) => {
    if (!backend.setCommentarySettings) return;
    const saved = await backend.setCommentarySettings(form);
    setSettings({ ...DEFAULTS, ...saved });
    if (key !== "" && backend.setCommentaryKey) setKeyStatus(await backend.setCommentaryKey(key, saved.remember_key ?? false));
    else await readKey();
  };

  const bindings: MenuBindings = { aiCommentary: { run: show } };

  /** The menu strip's AI 解说 button, pressed while the window is open. */
  const button = (texts: { aiCommentary: string; aiCommentaryTip: string }) => (
    <ToolButton label={texts.aiCommentary} title={texts.aiCommentaryTip} pressed={open} testId="commentary-button" onClick={open ? close : show} />
  );

  const windows = (
    <>
      {open ? (
        <CommentaryWindow
          labels={labels}
          mode={mode}
          onMode={setMode}
          current={current}
          range={range}
          onRange={setRange}
          configured={configured}
          entries={entries}
          failure={failure}
          model={settings.model}
          writeToComment={settings.write_to_comment}
          onWriteToComment={writeToComment}
          onStart={() => send(null)}
          onStop={stop}
          onAsk={send}
          onSettings={() => {
            setSetupOpen(true);
            void readKey().catch(report);
          }}
          onJump={place.jump}
          onClose={close}
        />
      ) : null}
      {setupOpen ? (
        <CommentarySetup
          labels={labels}
          settings={settings}
          keyStatus={keyStatus}
          describe={describe}
          onSave={connect}
          onModels={async (form, key) => {
            await connect(form, key);
            return backend.commentaryModels ? backend.commentaryModels() : [];
          }}
          onClearKey={async () => {
            if (backend.clearCommentaryKey) setKeyStatus(await backend.clearCommentaryKey());
          }}
          onClose={() => setSetupOpen(false)}
        />
      ) : null}
    </>
  );

  return { load, bindings, button, windows, busy, dialogOpen: setupOpen };
}
