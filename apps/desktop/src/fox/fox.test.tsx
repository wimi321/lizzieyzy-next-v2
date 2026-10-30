import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { FoxAccount, FoxGame, FoxPage, FoxResult, FoxSearchResult } from "@/api/backend";
import en from "../locales/en.json";
import zh from "../locales/zh-CN.json";
import { FoxDialog } from "./FoxDialog";
import { errorText, resultText, shortTime } from "./format";

afterEach(cleanup);

const me: FoxAccount = { uid: "100200", nickname: "棋迷甲" };

function result(winner: FoxResult["winner"], kind: FoxResult["kind"], score: number | null = null, unit: FoxResult["unit"] = null): FoxResult {
  return { winner, kind, score, unit };
}

function game(id: string, ownBlack = true): FoxGame {
  const own = { uid: me.uid, nickname: me.nickname, rank: "5段" };
  const other = { uid: `9${id}`, nickname: `对手${id}`, rank: "4段" };
  return {
    id,
    start_time: "2026-10-02 14:31:30",
    black: ownBlack ? own : other,
    white: ownBlack ? other : own,
    result: result("Black", "Resignation"),
    move_count: 211,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function dialog(overrides: Partial<Parameters<typeof FoxDialog>[0]> = {}) {
  const backend = {
    foxSearch: vi.fn<(query: unknown) => Promise<FoxSearchResult>>(),
    foxNextPage: vi.fn<(uid: string, cursor: string) => Promise<FoxPage>>(),
    foxOpenGame: vi.fn<(id: string) => Promise<number>>(),
    foxClearHistory: vi.fn<() => Promise<void>>(() => Promise.resolve()),
  };
  const props = { onHistoryChanged: vi.fn(), onClose: vi.fn() };
  render(<FoxDialog backend={backend} labels={zh.fox} history={[]} {...props} {...overrides} />);
  return { backend, ...props };
}

function searchFor(text: string) {
  fireEvent.change(screen.getByTestId("fox-input"), { target: { value: text } });
  fireEvent.click(screen.getByTestId("fox-search"));
}

test("results read like LizzieYzy Next's list", () => {
  expect(resultText(result("Black", "Resignation"), zh.fox.results)).toBe("黑中盘胜");
  expect(resultText(result("White", "Timeout"), zh.fox.results)).toBe("白超时胜");
  expect(resultText(result("White", "Score", 2.5, "Points"), zh.fox.results)).toBe("白胜 2.5 目");
  expect(resultText(result("Black", "Score", 1.25, "Stones"), zh.fox.results)).toBe("黑胜 1.25 子");
  expect(resultText(result("Black", "Score"), zh.fox.results)).toBe("黑胜");
  expect(resultText(result("White", "OtherWin"), zh.fox.results)).toBe("白胜");
  expect(resultText(result(null, "Other"), zh.fox.results)).toBe("无胜负");
  expect(resultText(result("White", "Score", 0.5, "Points"), en.fox.results)).toBe("W+0.5");
  expect(resultText(result("Black", "Resignation"), en.fox.results)).toBe("B+R");
});

test("times drop the seconds and unknown shapes pass through", () => {
  expect(shortTime("2026-10-02 14:31:30")).toBe("2026-10-02 14:31");
  expect(shortTime("2026-10-02T09:05:00Z")).toBe("2026-10-02 09:05");
  expect(shortTime("yesterday")).toBe("yesterday");
});

test("typed Fox failures become sentences; other errors keep their text", () => {
  const errors = zh.fox.errors;
  expect(errorText({ type: "Fox", data: { type: "UserNotFound" } }, errors)).toBe("没有找到这个野狐账号，请确认昵称");
  expect(errorText({ type: "Fox", data: { type: "Http", data: { status: 502 } } }, errors)).toBe("野狐服务器返回错误（HTTP 502）");
  expect(errorText({ type: "Fox", data: { type: "Api", data: { code: 7 } } }, errors)).toBe("野狐接口返回错误（代码 7）");
  expect(errorText({ type: "App", data: "engine stopped" }, errors)).toBe("engine stopped");
  expect(errorText(new Error("boom"), errors)).toBe("boom");
});

test("a nickname search lists the games with the account's side in bold", async () => {
  const { backend, onHistoryChanged } = dialog();
  backend.foxSearch.mockResolvedValue({ account: me, page: { games: [game("1"), game("2", false)], next_cursor: null } });

  searchFor("  棋迷甲 ");
  expect(backend.foxSearch).toHaveBeenCalledWith({ type: "Nickname", value: "棋迷甲" });
  expect(screen.getByTestId("fox-loading")).toBeTruthy();
  expect((screen.getByTestId("fox-search") as HTMLButtonElement).disabled).toBe(true);

  await act(async () => {});
  expect(screen.getByTestId("fox-account").textContent).toBe("棋迷甲 · UID 100200");
  const rows = screen.getAllByTestId("fox-game");
  expect(rows).toHaveLength(2);
  expect(rows[0]?.textContent).toContain("2026-10-02 14:31");
  expect(rows[0]?.textContent).toContain("黑中盘胜");
  expect(rows[0]?.querySelectorAll(".fox-player")[0]?.classList.contains("own")).toBe(true);
  expect(rows[1]?.querySelectorAll(".fox-player")[1]?.classList.contains("own")).toBe(true);
  expect(screen.getByText("没有更多棋谱了")).toBeTruthy();
  expect(onHistoryChanged).toHaveBeenCalledTimes(1);
});

test("UID mode searches by UID and a recent account searches by its UID", async () => {
  const { backend } = dialog({ history: [me, { uid: "300", nickname: "乙" }] });
  backend.foxSearch.mockResolvedValue({ account: me, page: { games: [], next_cursor: null } });

  fireEvent.click(screen.getByTestId("fox-mode-uid"));
  expect(screen.getByTestId("fox-input").getAttribute("placeholder")).toBe("输入野狐 UID（数字）");
  searchFor("42");
  expect(backend.foxSearch).toHaveBeenLastCalledWith({ type: "Uid", value: "42" });
  await act(async () => {});
  expect(screen.getByText("这个账号还没有公开棋谱")).toBeTruthy();

  fireEvent.click(screen.getAllByTestId("fox-recent")[1] as HTMLElement);
  expect(backend.foxSearch).toHaveBeenLastCalledWith({ type: "Uid", value: "300" });
  expect((screen.getByTestId("fox-input") as HTMLInputElement).value).toBe("300");
  await act(async () => {});
});

test("clearing the history tells the app to re-read it", async () => {
  const { backend, onHistoryChanged } = dialog({ history: [me] });
  fireEvent.click(screen.getByTestId("fox-clear-recent"));
  expect(backend.foxClearHistory).toHaveBeenCalled();
  await act(async () => {});
  expect(onHistoryChanged).toHaveBeenCalled();
});

test("load more appends the next page until the cursor runs out", async () => {
  const { backend } = dialog();
  backend.foxSearch.mockResolvedValue({ account: me, page: { games: [game("1")], next_cursor: "c1" } });
  const next = deferred<FoxPage>();
  backend.foxNextPage.mockReturnValue(next.promise);

  searchFor("棋迷甲");
  await act(async () => {});
  fireEvent.click(screen.getByTestId("fox-more"));
  expect(backend.foxNextPage).toHaveBeenCalledWith("100200", "c1");
  expect(screen.getByTestId("fox-more").textContent).toBe("正在加载…");

  await act(async () => next.resolve({ games: [game("2"), game("3")], next_cursor: null }));
  expect(screen.getAllByTestId("fox-game")).toHaveLength(3);
  expect(screen.queryByTestId("fox-more")).toBeNull();
});

test("opening a game shows progress and closes the window by default", async () => {
  const { backend, onClose } = dialog();
  backend.foxSearch.mockResolvedValue({ account: me, page: { games: [game("1"), game("2")], next_cursor: null } });
  const opened = deferred<number>();
  backend.foxOpenGame.mockReturnValue(opened.promise);

  searchFor("棋迷甲");
  await act(async () => {});
  fireEvent.click(screen.getAllByTestId("fox-game")[1] as HTMLElement);
  expect(backend.foxOpenGame).toHaveBeenCalledWith("2");
  expect(screen.getAllByTestId("fox-game")[1]?.textContent).toContain("正在打开…");
  expect((screen.getAllByTestId("fox-game")[0] as HTMLButtonElement).disabled).toBe(true);

  await act(async () => opened.resolve(7));
  expect(onClose).toHaveBeenCalled();
});

test("with close-after-open unticked the window stays and a failed open is reported", async () => {
  const { backend, onClose } = dialog();
  backend.foxSearch.mockResolvedValue({ account: me, page: { games: [game("1")], next_cursor: null } });
  backend.foxOpenGame.mockResolvedValueOnce(3).mockRejectedValueOnce({ type: "Fox", data: { type: "DownloadFailed" } });

  searchFor("棋迷甲");
  await act(async () => {});
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByTestId("fox-game"));
  await act(async () => {});
  expect(onClose).not.toHaveBeenCalled();

  fireEvent.click(screen.getByTestId("fox-game"));
  await act(async () => {});
  expect(screen.getByTestId("fox-open-error").textContent).toBe("棋谱下载失败，请重试");
  expect(screen.getAllByTestId("fox-game")).toHaveLength(1);
});

test("a failed search says why and allows another try", async () => {
  const { backend } = dialog();
  backend.foxSearch.mockRejectedValueOnce({ type: "Fox", data: { type: "Timeout" } });

  searchFor("棋迷甲");
  await act(async () => {});
  expect(screen.getByTestId("fox-error").textContent).toBe("野狐服务器响应太慢，请稍后重试");
  expect((screen.getByTestId("fox-search") as HTMLButtonElement).disabled).toBe(false);
});

test("the close button closes", () => {
  const { onClose } = dialog();
  fireEvent.click(screen.getByTestId("fox-close"));
  expect(onClose).toHaveBeenCalled();
});
