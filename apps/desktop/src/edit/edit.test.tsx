import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { DocumentView, GameInfoEdit, VariationTreeView } from "@/api/backend";
import { documentView, midGame } from "../../e2e/fixtures";
import zh from "../locales/zh-CN.json";
import { menuBindings, type MenuActions, type MenuState } from "../menu/bindings";
import { matchShortcut } from "../menu/shortcuts";
import { labelOf, MENUS, type MenuEntry, withEditing } from "../menu/spec";
import { MoveTree } from "../panels/MoveTree";
import { NotesPanel } from "../panels/NotesPanel";
import { BoardSizeDialog } from "./BoardSizeDialog";
import { GameInfoDialog } from "./GameInfoDialog";
import { editedText, infoEdit, saveRoute } from "./info";
import { useCommentEdit } from "./useCommentEdit";
import { useEditing } from "./useEditing";

afterEach(cleanup);

function game(change: (view: DocumentView) => void = () => {}): DocumentView {
  const view = midGame();
  view.comment = "原来的评论";
  change(view);
  return view;
}

describe("editing a comment", () => {
  const backend = () => ({ setComment: vi.fn(() => Promise.resolve(8)) });

  test("leaving the editor saves a changed comment, without the line break typed last", () => {
    const fake = backend();
    const { result } = renderHook(({ view }) => useCommentEdit(fake, view), { initialProps: { view: game() } });
    expect(result.current.text).toBeNull();
    act(() => result.current.begin());
    expect(result.current.text).toBe("原来的评论");
    act(() => result.current.change("新的评论\n"));
    act(() => result.current.commit());
    expect(fake.setComment).toHaveBeenCalledWith("新的评论");
    expect(result.current.text).toBeNull();
    // Nothing is being edited any more: a second commit, as from a late blur, does nothing.
    act(() => result.current.commit());
    expect(fake.setComment).toHaveBeenCalledTimes(1);
  });

  test("an unchanged comment sends nothing; clearing it removes it; Escape gives it up", () => {
    const fake = backend();
    const { result } = renderHook(() => useCommentEdit(fake, game()));
    act(() => result.current.begin());
    act(() => result.current.commit());
    expect(fake.setComment).not.toHaveBeenCalled();

    act(() => result.current.begin());
    act(() => result.current.change("改了又不要"));
    act(() => result.current.cancel());
    act(() => result.current.commit());
    expect(fake.setComment).not.toHaveBeenCalled();

    act(() => result.current.begin());
    act(() => result.current.change(""));
    act(() => result.current.commit());
    expect(fake.setComment).toHaveBeenCalledWith("");
  });

  test("the app's own navigation commits first, to the move the text was written for", () => {
    const fake = backend();
    const { result, rerender } = renderHook(({ view }) => useCommentEdit(fake, view), { initialProps: { view: game() } });
    act(() => result.current.begin());
    act(() => result.current.change("翻手之前"));
    // step() and jump() call commit() before they ask Rust to move.
    act(() => result.current.commit());
    expect(fake.setComment).toHaveBeenCalledWith("翻手之前");
    rerender({ view: game((view) => (view.tree.current = 21)) });
    expect(result.current.text).toBeNull();
  });

  test("when the game changes under the editor the unsaved text is dropped, never written elsewhere", () => {
    const fake = backend();
    const { result, rerender } = renderHook(({ view }) => useCommentEdit(fake, view), { initialProps: { view: game() } });
    act(() => result.current.begin());
    act(() => result.current.change("没来得及保存"));
    // Another game was opened: same node id, another tree.
    rerender({ view: game((view) => (view.tree.structure_revision = 9)) });
    expect(result.current.text).toBeNull();
    act(() => result.current.commit());
    expect(fake.setComment).not.toHaveBeenCalled();

    // The board was synchronized to another move.
    act(() => result.current.begin());
    act(() => result.current.change("又没保存"));
    rerender({ view: game((view) => ((view.tree.structure_revision = 9), (view.tree.current = 5))) });
    act(() => result.current.commit());
    expect(fake.setComment).not.toHaveBeenCalled();
  });

  test("the panel swaps the comment for an editor: Ctrl+Enter and blur commit, Escape cancels", () => {
    const editor = { text: "写到一半", onChange: vi.fn(), onCommit: vi.fn(), onCancel: vi.fn() };
    const onEdit = vi.fn();
    const panel = (editing: boolean) => (
      <NotesPanel tab="comment" onTab={vi.fn()} problemCount={0} status={null} summary={["白棋 胜率: 38.0%"]} comment="原来的评论" editor={editing ? editor : null} onEdit={onEdit} labels={zh.notes}>
        <p />
      </NotesPanel>
    );
    const { rerender } = render(panel(false));
    fireEvent.click(screen.getByTestId("comment-text"));
    expect(onEdit).toHaveBeenCalledTimes(1);

    rerender(panel(true));
    const area = screen.getByTestId("comment-editor") as HTMLTextAreaElement;
    // The analysis note stays above; only the game's own comment is edited.
    expect(area.value).toBe("写到一半");
    expect(screen.getByTestId("analysis-summary").textContent).toBe("白棋 胜率: 38.0%");
    fireEvent.change(area, { target: { value: "写完了" } });
    expect(editor.onChange).toHaveBeenCalledWith("写完了");
    fireEvent.keyDown(area, { key: "Enter", ctrlKey: true });
    fireEvent.blur(area);
    expect(editor.onCommit).toHaveBeenCalledTimes(2);
    fireEvent.keyDown(area, { key: "Escape" });
    expect(editor.onCancel).toHaveBeenCalledTimes(1);
  });
});

describe("saving, game info and new boards", () => {
  test("保存 writes back, asks first, or asks where", () => {
    expect(saveRoute("/games/a.sgf", true)).toBe("confirm");
    expect(saveRoute("/games/a.sgf", false)).toBe("write");
    expect(saveRoute(null, true)).toBe("choose");
    expect(saveRoute(undefined, false)).toBe("choose");
  });

  test("a field left alone keeps its recorded value; a cleared one is removed", () => {
    expect(editedText(null, "")).toBeNull();
    expect(editedText("", "")).toBe("");
    expect(editedText("甲", "甲")).toBe("甲");
    expect(editedText("甲", "  ")).toBeNull();
    expect(editedText(null, "乙")).toBe("乙");
    const view = game((v) => ((v.game_info.black_name = "甲"), (v.game_info.rules = "Chinese"), (v.game_info.rules_preset = "chinese")));
    expect(infoEdit(view, { komi: 6.5 })).toEqual({
      black_name: "甲", black_rank: null, white_name: null, white_rank: null, komi: 6.5, rules: "chinese", result: null, date: null, event: null,
    });
  });

  function editingBackend() {
    return {
      newGame: vi.fn(() => Promise.resolve(1)),
      setGameInfo: vi.fn((edit: GameInfoEdit) => Promise.resolve(edit.komi)),
      save: vi.fn(() => Promise.resolve()),
      pickSavePath: vi.fn(() => Promise.resolve<string | null>("/games/new.sgf")),
      openPath: vi.fn(() => Promise.resolve(1)),
      recentFiles: vi.fn(() => Promise.resolve(["/games/a.sgf", "/games/b.sgf"])),
      clearRecentFiles: vi.fn(() => Promise.resolve()),
      deleteNode: vi.fn(() => Promise.resolve(1)),
      deleteBranch: vi.fn(() => Promise.resolve(1)),
      promoteToMain: vi.fn(() => Promise.resolve(1)),
      undo: vi.fn(() => Promise.resolve(1)),
      redo: vi.fn(() => Promise.resolve(1)),
      startTrial: vi.fn(() => Promise.resolve(1)),
      endTrial: vi.fn(() => Promise.resolve(1)),
    };
  }

  function Harness({ fake, view, empty = false }: { fake: ReturnType<typeof editingBackend>; view: DocumentView; empty?: boolean }) {
    const [confirms, setConfirms] = useState({ newBoard: true, replace: true });
    const editing = useEditing(fake, view, empty, zh.edit, zh.info.rules, null, confirms, (kind) => setConfirms((value) => ({ ...value, [kind]: false })));
    return (
      <>
        <button data-testid="new" onClick={editing.newBoard} />
        <button data-testid="save" onClick={editing.save} />
        <button data-testid="komi" onClick={() => editing.setKomi(6.5)} />
        <button data-testid="delete" onClick={editing.deleteMove} />
        <output data-testid="recent">{editing.recent.join("|")}</output>
        <output data-testid="asks">{`${confirms.newBoard}/${confirms.replace}`}</output>
        {editing.dialogs}
      </>
    );
  }

  test("a new board keeps the size, komi and rules, and asks before it clears a game", async () => {
    const fake = editingBackend();
    const view = game((v) => ((v.game_info.komi = 6.5), (v.game_info.rules_preset = "japanese")));
    const { rerender } = render(<Harness fake={fake} view={view} />);
    expect((await screen.findByTestId("recent")).textContent).toBe("/games/a.sgf|/games/b.sgf");

    fireEvent.click(screen.getByTestId("new"));
    expect(fake.newGame).not.toHaveBeenCalled();
    expect(screen.getByTestId("confirm-new-board").textContent).toContain(zh.edit.newBoardMessage);
    fireEvent.click(screen.getByTestId("confirm-new-board-cancel"));
    expect(screen.queryByTestId("confirm-new-board")).toBeNull();
    expect(fake.newGame).not.toHaveBeenCalled();

    // "Do not ask again" holds from then on.
    fireEvent.click(screen.getByTestId("new"));
    fireEvent.click(screen.getByTestId("confirm-new-board-no-again"));
    fireEvent.click(screen.getByTestId("confirm-new-board-ok"));
    const options = { width: 19, height: 19, komi: 6.5, handicap: 0, rules: "japanese", black_name: null, black_rank: null, white_name: null, white_rank: null };
    expect(fake.newGame).toHaveBeenCalledWith(options);
    expect(screen.getByTestId("asks").textContent).toBe("false/true");
    fireEvent.click(screen.getByTestId("new"));
    expect(fake.newGame).toHaveBeenCalledTimes(2);

    // An empty game has nothing to lose: no question, even with the switch back on.
    const fresh = editingBackend();
    rerender(<Harness fake={fresh} view={documentView(view.board!, 0)} empty />);
    fireEvent.click(screen.getByTestId("new"));
    expect(screen.queryByTestId("confirm-new-board")).toBeNull();
    expect(fresh.newGame).toHaveBeenCalledTimes(1);
  });

  test("保存 asks before it writes over the game's file; a game without a file asks where", async () => {
    const fake = editingBackend();
    const view = game((v) => (v.file_path = "/games/a.sgf"));
    const { rerender } = render(<Harness fake={fake} view={view} />);
    fireEvent.click(screen.getByTestId("save"));
    expect(fake.save).not.toHaveBeenCalled();
    expect(screen.getByTestId("confirm-replace").textContent).toContain("/games/a.sgf");
    fireEvent.click(screen.getByTestId("confirm-replace-no-again"));
    fireEvent.click(screen.getByTestId("confirm-replace-ok"));
    expect(fake.save).toHaveBeenCalledWith("/games/a.sgf");
    fireEvent.click(screen.getByTestId("save"));
    expect(fake.save).toHaveBeenCalledTimes(2);
    expect(fake.pickSavePath).not.toHaveBeenCalled();

    rerender(<Harness fake={fake} view={game()} />);
    await act(async () => fireEvent.click(screen.getByTestId("save")));
    expect(fake.pickSavePath).toHaveBeenCalledTimes(1);
    expect(fake.save).toHaveBeenLastCalledWith("/games/new.sgf");
  });

  test("the toolbar's komi and the delete button send the edit for the current game", () => {
    const fake = editingBackend();
    render(<Harness fake={fake} view={game()} />);
    fireEvent.click(screen.getByTestId("komi"));
    expect(fake.setGameInfo).toHaveBeenCalledWith(expect.objectContaining({ komi: 6.5, rules: "chinese", black_name: null }));
    fireEvent.click(screen.getByTestId("delete"));
    expect(fake.deleteNode).toHaveBeenCalledWith(20);
  });

  test("the game-info dialog sends every field, changed or not, and shows why it failed", async () => {
    const info = game((v) => {
      v.game_info = { ...v.game_info, black_name: "甲", white_name: "乙", komi: 7.5, rules: "Chinese", rules_preset: "chinese", result: "B+R", date: null, event: "" };
    }).game_info;
    const onSave = vi.fn((edit: GameInfoEdit) => (edit.komi > 100 ? Promise.reject({ type: "App", data: "invalid KM" }) : Promise.resolve()));
    render(<GameInfoDialog info={info} labels={zh.edit} ruleNames={zh.info.rules} onSave={onSave} onClose={vi.fn()} />);
    expect((screen.getByTestId("game-info-rules") as HTMLSelectElement).value).toBe("chinese");
    fireEvent.change(screen.getByTestId("game-info-black_name"), { target: { value: "" } });
    fireEvent.change(screen.getByTestId("game-info-white_rank"), { target: { value: "9段" } });
    fireEvent.change(screen.getByTestId("game-info-rules"), { target: { value: "japanese" } });
    fireEvent.change(screen.getByTestId("game-info-komi"), { target: { value: "abc" } });
    expect((screen.getByTestId("game-info-confirm") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByTestId("game-info-komi"), { target: { value: "6.5" } });
    await act(async () => fireEvent.click(screen.getByTestId("game-info-confirm")));
    expect(onSave).toHaveBeenCalledWith({
      black_name: null, black_rank: null, white_name: "乙", white_rank: "9段", komi: 6.5, rules: "japanese", result: "B+R", date: null, event: "",
    });

    fireEvent.change(screen.getByTestId("game-info-komi"), { target: { value: "500" } });
    await act(async () => fireEvent.click(screen.getByTestId("game-info-confirm")));
    expect(screen.getByTestId("game-info-error").textContent).toContain("invalid KM");
  });

  test("the board-size dialog offers Java's sizes and any width by height the rules allow", () => {
    const onConfirm = vi.fn();
    render(<BoardSizeDialog width={19} height={19} labels={zh.edit} onConfirm={onConfirm} onClose={vi.fn()} />);
    expect(screen.getByTestId("board-size-19").getAttribute("aria-checked")).toBe("true");
    fireEvent.click(screen.getByTestId("board-size-13"));
    fireEvent.click(screen.getByTestId("board-size-confirm"));
    expect(onConfirm).toHaveBeenLastCalledWith(13, 13);

    fireEvent.click(screen.getByTestId("board-size-other"));
    fireEvent.change(screen.getByTestId("board-size-width"), { target: { value: "9" } });
    fireEvent.change(screen.getByTestId("board-size-height"), { target: { value: "30" } });
    expect((screen.getByTestId("board-size-confirm") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByTestId("board-size-height"), { target: { value: "13" } });
    fireEvent.click(screen.getByTestId("board-size-confirm"));
    expect(onConfirm).toHaveBeenLastCalledWith(9, 13);
  });
});

describe("the menu's editing entries", () => {
  const tree: VariationTreeView = {
    revision: 1,
    root: 0,
    nodes: [
      { id: 0, parent: null, children: [1], move_number: 0, mv: null, move_error: null },
      { id: 1, parent: 0, children: [2, 3], move_number: 1, mv: null, move_error: null },
      { id: 2, parent: 1, children: [], move_number: 2, mv: null, move_error: null },
      { id: 3, parent: 1, children: [], move_number: 2, mv: null, move_error: null, has_comment: true },
    ],
  };
  const state = (change: Partial<MenuState>): MenuState => ({
    hasDocument: true, hasBoard: true, ownership: false, wholeGameActive: false, hasBestMove: false, engineFailed: false,
    engineConfigured: true, windows: false, syncPhase: "Stopped", syncOptions: { always_sync: true, always_goto_last: false },
    tree, current: 0, canUndo: false, canRedo: false, recent: [], setupTool: null, setupEditable: false, trial: false, playColor: null, insertColor: null, ...change,
  });
  const actions = () => new Proxy({}, { get: (target: Record<string, unknown>, key: string) => (target[key] ??= vi.fn()) }) as unknown as MenuActions;

  test("最近打开 lists file names with their paths on hover, then 清空列表; empty, it says so", () => {
    const text = zh.edit;
    const find = (menus: readonly MenuEntry[], id: string): MenuEntry | undefined => {
      for (const entry of menus) {
        if (entry.kind === "separator") continue;
        if (entry.id === id) return entry;
        const inner = entry.kind === "submenu" ? find(entry.items, id) : undefined;
        if (inner) return inner;
      }
      return undefined;
    };
    const filled = find(withEditing(MENUS, ["/games/甲 vs 乙.sgf", "C:\\kifu\\b.sgf"], text), "openRecent");
    expect(filled?.kind === "submenu" && filled.items.map((entry) => (entry.kind === "separator" ? "-" : `${entry.id}:${labelOf(entry, zh.menu)}`))).toEqual([
      "recent0:甲 vs 乙.sgf",
      "recent1:b.sgf",
      "-",
      "clearRecent:清空列表",
    ]);
    expect(filled?.kind === "submenu" && filled.items[0]?.kind === "item" && filled.items[0].title).toBe("/games/甲 vs 乙.sgf");
    const empty = find(withEditing(MENUS, [], text), "openRecent");
    expect(empty?.kind === "submenu" && empty.items.map((entry) => entry.kind !== "separator" && labelOf(entry, zh.menu))).toEqual([text.recentEmpty]);
    // Java's 撤销删除 and 恢复删除 undo and redo any edit here, and say so.
    const menus = withEditing(MENUS, [], text);
    expect(labelOf(find(menus, "undoDelete") as Exclude<MenuEntry, { kind: "separator" }>, zh.menu)).toBe("撤销编辑");
    expect(labelOf(find(menus, "redoDelete") as Exclude<MenuEntry, { kind: "separator" }>, zh.menu)).toBe("恢复编辑");
  });

  test("entries wait for what they need: a move to delete, a branch to promote, an edit to undo", () => {
    const run = actions();
    const root = menuBindings(state({}), run);
    expect(root.delete?.enabled).toBe(false);
    expect(root.deleteBranch?.enabled).toBe(false);
    expect(root.setAsMain?.enabled).toBe(false);
    expect(root.undoDelete?.enabled).toBe(false);
    expect(root.clearRecent).toBeUndefined();

    const main = menuBindings(state({ current: 2, canUndo: true }), run);
    expect(main.delete?.enabled).toBe(true);
    expect(main.setAsMain?.enabled).toBe(false);
    expect(main.undoDelete?.enabled).toBe(true);
    expect(main.redoDelete?.enabled).toBe(false);

    const branch = menuBindings(state({ current: 3, recent: ["/a.sgf", "/b.sgf"] }), run);
    expect(branch.setAsMain?.enabled).toBe(true);
    branch.recent1?.run?.();
    expect(run.openRecent).toHaveBeenCalledWith("/b.sgf");
    branch.clearRecent?.run?.();
    expect(run.clearRecent).toHaveBeenCalledTimes(1);
    branch.clearBoard?.run?.();
    branch.newBoard?.run?.();
    expect(run.newBoard).toHaveBeenCalledTimes(2);
  });

  test("Java's keys reach them", () => {
    const press = (key: Partial<KeyboardEvent>) => matchShortcut({ key: "", code: "", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false, ...key });
    expect(press({ code: "Delete" })).toBe("delete");
    expect(press({ code: "Backspace" })).toBe("delete");
    expect(press({ code: "Delete", shiftKey: true })).toBe("deleteBranch");
    expect(press({ code: "KeyL" })).toBe("setAsMain");
    expect(press({ code: "KeyI" })).toBe("setInfo");
    expect(press({ code: "KeyI", ctrlKey: true })).toBe("setBoard");
    expect(press({ code: "Home", metaKey: true })).toBe("clearBoard");
    expect(press({ code: "Home" })).toBe("jumpToFirst");
    expect(press({ code: "KeyS", ctrlKey: true })).toBe("save");
  });

  test("the tree rings a move that has a comment", () => {
    render(<MoveTree tree={tree} current={3} line={[0, 1, 3]} boardSize={19} label="棋谱" onJump={vi.fn()} />);
    const nodes = screen.getAllByTestId("tree-node");
    expect(nodes.filter((node) => node.dataset.comment === "true").map((node) => node.dataset.id)).toEqual(["3"]);
    expect(screen.getAllByTestId("tree-comment")).toHaveLength(1);
  });
});
