import { useCallback, useEffect, useRef, useState } from "react";
import type { Backend, BoardTransform, BoardView, Settings } from "@/api/backend";
import { NumberDialog, type NumberDialogLabels } from "../display/NumberDialog";
import { MessageBox } from "../frame/MessageBox";
import type { MenuBinding } from "../menu/bindings";
import { canvasPng, svgPng } from "./capture";
import { playStone } from "./sound";

export interface EverydayLabels {
  failedTitle: string;
  copySgf: string;
  paste: string;
  pasteEmpty: string;
  saveSgf: string;
  saveImage: string;
  copyImage: string;
  transform: string;
  setting: string;
  ok: string;
  fontTitle: string;
  fontPrompt: string;
}

type EverydayBackend = Pick<
  Backend,
  | "copySgf"
  | "pasteSgf"
  | "saveCleanSgf"
  | "savePng"
  | "copyPng"
  | "transformBoard"
  | "setAlwaysOnTop"
  | "setUiScale"
  | "setPlaySound"
  | "pickSavePath"
  | "pickImagePath"
>;

export interface EverydayView {
  board: BoardView | null;
  fileName: string | null;
  /** The panels a picture can be taken of are on screen. */
  subBoard: boolean;
  graph: boolean;
}

/** 设置 → 界面字号: Java's 小/中/大 (12, 16, 20 pt) as a zoom of the whole window. */
export const FONT_SCALES = { frameFontSizeSmall: 1, frameFontSizeMiddle: 1.15, frameFontSizeBig: 1.3 } as const;

type Failure = { what: Exclude<keyof EverydayLabels, "failedTitle" | "ok" | "fontTitle" | "fontPrompt">; reason: string };

/** A typed backend error names its cause; nested ones (a clipboard error inside a command error) are unwrapped. */
export function reasonOf(error: unknown): string {
  if (typeof error === "object" && error !== null && "type" in error) {
    const { type, data } = error as { type: string; data?: unknown };
    if (typeof data === "string") return data;
    if (typeof data === "object" && data !== null) return reasonOf(data);
    return type;
  }
  return error instanceof Error ? error.message : String(error);
}

function emptyClipboard(error: unknown): boolean {
  const typed = error as { type?: string; data?: { type?: string } } | null;
  return typed?.type === "Clipboard" && typed.data?.type === "EmptyText";
}

const near = (a: number, b: number) => Math.abs(a - b) < 0.005;

/**
 * P3-03: 文件 → 复制/粘贴棋谱、更多保存、复制棋盘; 编辑 → 旋转/翻转; 显示 → 总在最前;
 * 设置 → 界面字号、播放落子声; 文件 → 加载棋谱中的贴目.
 */
export function useEveryday(backend: EverydayBackend, view: EverydayView, readKomi: { value: boolean; set(enabled: boolean): void }) {
  const [onTop, setOnTop] = useState(false);
  const [scale, setScale] = useState(1);
  const [sound, setSound] = useState(true);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [scaleOpen, setScaleOpen] = useState(false);

  const load = useCallback((settings: Settings) => {
    setOnTop(settings.always_on_top ?? false);
    setScale(settings.ui_scale ?? 1);
    setSound(settings.play_sound ?? true);
  }, []);

  // The whole window grows with the font: Java's larger sizes also made its panels larger.
  useEffect(() => {
    document.documentElement.style.zoom = near(scale, 1) ? "" : String(scale);
  }, [scale]);

  // A stone sounds when one more move is on the board: played, or stepped to.
  const board = view.board;
  const lastMoves = useRef<number | null>(null);
  useEffect(() => {
    const moves = board?.move_number ?? null;
    const before = lastMoves.current;
    lastMoves.current = moves;
    if (sound && before !== null && moves === before + 1 && board?.last_move?.type === "Play") playStone();
  }, [board, sound]);

  const act = (what: Failure["what"], task: () => Promise<unknown>) => () =>
    void task().catch((error: unknown) =>
      setFailure(what === "paste" && emptyClipboard(error) ? { what: "pasteEmpty", reason: "" } : { what, reason: reasonOf(error) }),
    );

  const stem = () => (view.fileName ?? "LizzieYzy").replace(/\.[^./\\]+$/, "");
  const saveClean = (keepComments: boolean, branch: boolean) => async () => {
    const path = await backend.pickSavePath(`${stem()}-${branch ? "branch" : "raw"}.sgf`);
    if (path) await backend.saveCleanSgf(path, keepComments, branch);
  };
  // The picture is taken before the dialog opens, so it shows what was on screen.
  const saveImage = (capture: () => Promise<Uint8Array>, suffix: string) => async () => {
    const bytes = await capture();
    const path = await backend.pickImagePath(`${stem()}-${suffix}.png`);
    if (path) await backend.savePng(path, bytes);
  };
  const copyImage = (capture: () => Promise<Uint8Array>) => async () => backend.copyPng(await capture());
  const transform = (kind: BoardTransform) => act("transform", () => backend.transformBoard(kind));

  const setting = <T,>(apply: (value: T) => void, save: (value: T) => Promise<T>) => (next: T, previous: T) => {
    apply(next);
    save(next)
      .then(apply)
      .catch((error: unknown) => {
        apply(previous);
        setFailure({ what: "setting", reason: reasonOf(error) });
      });
  };
  const chooseOnTop = setting(setOnTop, (enabled: boolean) => backend.setAlwaysOnTop(enabled));
  const chooseScale = setting(setScale, (value: number) => backend.setUiScale(value));
  const chooseSound = setting(setSound, (enabled: boolean) => backend.setPlaySound(enabled));

  const hasBoard = board !== null;
  const square = board !== null && board.width === board.height;
  const moveNumber = String(board?.move_number ?? 0);
  const preset = Object.values(FONT_SCALES).some((value) => near(value, scale));

  const bindings: Record<string, MenuBinding> = {
    copySgf: { run: act("copySgf", () => backend.copySgf()), enabled: hasBoard },
    pasteSgf: { run: act("paste", () => backend.pasteSgf()) },
    saveRaw: { run: act("saveSgf", saveClean(false, false)), enabled: hasBoard },
    saveCommentRaw: { run: act("saveSgf", saveClean(true, false)), enabled: hasBoard },
    saveBranchRaw: { run: act("saveSgf", saveClean(false, true)), enabled: hasBoard },
    saveMainBoardScreen: { run: act("saveImage", saveImage(() => canvasPng("board"), moveNumber)), enabled: hasBoard },
    saveSubBoardScreen: { run: act("saveImage", saveImage(() => canvasPng("sub-board"), `${moveNumber}-sub`)), enabled: hasBoard && view.subBoard },
    saveWinrate: { run: act("saveImage", saveImage(() => svgPng("winrate-graph"), "winrate")), enabled: hasBoard && view.graph },
    copyBoardScreen: { run: act("copyImage", copyImage(() => canvasPng("board"))), enabled: hasBoard },
    copySubBoardScreen: { run: act("copyImage", copyImage(() => canvasPng("sub-board"))), enabled: hasBoard && view.subBoard },
    // Java turns only square boards; flipping works on any shape.
    spinRight: { run: transform("RotateRight"), enabled: square },
    spinLeft: { run: transform("RotateLeft"), enabled: square },
    mirrorVertical: { run: transform("FlipHorizontal"), enabled: hasBoard },
    mirrorHorizon: { run: transform("FlipVertical"), enabled: hasBoard },
    alwaysOnTop: { run: () => chooseOnTop(!onTop, onTop), checked: onTop },
    frameFontSizeSmall: { run: () => chooseScale(FONT_SCALES.frameFontSizeSmall, scale), checked: near(scale, FONT_SCALES.frameFontSizeSmall) },
    frameFontSizeMiddle: { run: () => chooseScale(FONT_SCALES.frameFontSizeMiddle, scale), checked: near(scale, FONT_SCALES.frameFontSizeMiddle) },
    frameFontSizeBig: { run: () => chooseScale(FONT_SCALES.frameFontSizeBig, scale), checked: near(scale, FONT_SCALES.frameFontSizeBig) },
    frameFontSizeOther: { run: () => setScaleOpen(true), checked: !preset },
    playSound: { run: () => chooseSound(!sound, sound), checked: sound },
    loadKomi: { run: () => readKomi.set(!readKomi.value), checked: readKomi.value },
  };

  const dialogs = (labels: EverydayLabels, numberLabels: NumberDialogLabels) => (
    <>
      {failure ? (
        <MessageBox
          title={labels.failedTitle}
          lines={[labels[failure.what], failure.reason].filter((line) => line !== "")}
          ok={labels.ok}
          testId="everyday-failed"
          onClose={() => setFailure(null)}
        />
      ) : null}
      {scaleOpen ? (
        <NumberDialog
          title={labels.fontTitle}
          prompt={labels.fontPrompt}
          value={Math.round(scale * 100)}
          min={50}
          max={300}
          labels={numberLabels}
          testId="font-scale"
          onConfirm={(percent) => {
            setScaleOpen(false);
            chooseScale(percent / 100, scale);
          }}
          onClose={() => setScaleOpen(false)}
        />
      ) : null}
    </>
  );

  return { load, bindings, dialogs, dialogOpen: scaleOpen };
}
