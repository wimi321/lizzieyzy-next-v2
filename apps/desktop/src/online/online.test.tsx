import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { TencentPage } from "../api/tencent";
import en from "../locales/en.json";
import { isQueried, playerText, rankText, startTime, tencentError, urlError } from "./format";
import { OpenUrlDialog } from "./OpenUrlDialog";
import { TencentDialog } from "./TencentDialog";

afterEach(cleanup);
const labels = en.online;

test("Tencent ranks, players and times read as Java lists them", () => {
  expect(rankText({ type: "Dan", data: 5 }, labels.tencent)).toBe("5d");
  expect(rankText({ type: "Kyu", data: 3 }, labels.tencent)).toBe("3k");
  expect(rankText({ type: "Professional", data: 2 }, labels.tencent)).toBe("P2d");
  expect(rankText(undefined, labels.tencent)).toBe("");
  expect(playerText({ nickname: "样本棋手910001", uid: "910001" })).toBe("样本棋手910001 (910001)");
  expect(playerText({ uid: "910002" })).toBe("910002");
  expect(isQueried({ nickname: "a", uid: "7" }, "7")).toBe(true);
  expect(isQueried({ nickname: "a", uid: "7" }, "a")).toBe(true);
  expect(isQueried({ nickname: "a", uid: "7" }, "")).toBe(false);
  const local = new Date(2026, 9, 2, 14, 31, 30);
  expect(startTime(local.getTime() / 1000)).toBe("2026-10-02 14:31");
  expect(startTime(null)).toBe("");
});

test("Tencent and link failures each get their own words", () => {
  const tencent = labels.tencent.errors;
  expect(tencentError({ type: "Tencent", data: { type: "InvalidQuery" } }, tencent)).toBe(tencent.InvalidQuery);
  expect(tencentError({ type: "Tencent", data: { type: "Api", data: { result: 3, ret: null, message: "busy" } } }, tencent)).toBe(
    "The Tencent service returned an error: busy",
  );
  expect(tencentError({ type: "Tencent", data: { type: "Request", data: { type: "Http", data: { status: 502 } } } }, tencent)).toBe(
    "The Tencent servers returned an error (HTTP 502)",
  );
  expect(tencentError({ type: "Tencent", data: { type: "Request", data: { type: "Timeout" } } }, tencent)).toBe(tencent.Timeout);
  const url = labels.url.errors;
  expect(urlError({ type: "OpenUrl", data: { type: "UnsupportedYike" } }, url)).toBe(url.UnsupportedYike);
  expect(urlError({ type: "OpenUrl", data: { type: "InvalidSgf" } }, url)).toBe(url.InvalidSgf);
  expect(urlError({ type: "OpenUrl", data: { type: "Request", data: { type: "ResponseTooLarge" } } }, url)).toBe(url.ResponseTooLarge);
  expect(urlError({ type: "OpenUrl", data: { type: "Request", data: { type: "DownloadFailed" } } }, url)).toBe(url.Request);
  expect(urlError(new Error("boom"), url)).toBe("boom");
});

function page(cursor: string | null, ...ids: string[]): TencentPage {
  return {
    next_cursor: cursor,
    games: ids.map((id, index) => ({
      id,
      start_time: 1_790_000_000 + index,
      black: { nickname: "样本棋手910001", uid: "910001", rank: { type: "Dan", data: 5 } },
      white: { nickname: "样本棋手910002", uid: "910002", rank: { type: "Kyu", data: 1 } },
      result: { winner: index === 0 ? "Black" : "White", kind: "Resignation", score: null, unit: null },
      move_count: 200 + index,
    })),
  };
}

describe("TencentDialog", () => {
  function setup(history: string[] = []) {
    const backend = {
      tencentSearch: vi.fn(() => Promise.resolve(page("810002", "810001", "810002"))),
      tencentNextPage: vi.fn(() => Promise.resolve(page(null, "810003"))),
      tencentOpenGame: vi.fn(() => Promise.resolve(5)),
      tencentClearHistory: vi.fn(() => Promise.resolve()),
    };
    const onClose = vi.fn();
    const onHistoryChanged = vi.fn();
    render(<TencentDialog open backend={backend} labels={labels.tencent} results={en.fox.results} history={history} onHistoryChanged={onHistoryChanged} onClose={onClose} />);
    return { backend, onClose, onHistoryChanged };
  }

  test("searching lists the games in Java's nine columns; the account's wins stand out", async () => {
    const { backend, onHistoryChanged } = setup();
    fireEvent.change(screen.getByTestId("tencent-input"), { target: { value: " 样本棋手910001 " } });
    await act(async () => fireEvent.click(screen.getByTestId("tencent-search")));
    expect(backend.tencentSearch).toHaveBeenCalledWith("样本棋手910001");
    expect(onHistoryChanged).toHaveBeenCalled();
    const rows = screen.getAllByTestId("tencent-game");
    expect(rows).toHaveLength(2);
    expect(within(rows[0]!).getAllByRole("cell").map((cell) => cell.textContent)).toEqual([
      "1",
      startTime(1_790_000_000),
      "样本棋手910001 (910001)",
      "5d",
      "样本棋手910002 (910002)",
      "1k",
      "B+R",
      "200",
      "Open",
    ]);
    expect(rows[0]!.className).toContain("won");
    expect(rows[1]!.className).not.toContain("won");

    await act(async () => fireEvent.click(screen.getByTestId("tencent-more")));
    expect(backend.tencentNextPage).toHaveBeenCalledWith("样本棋手910001", "810002");
    expect(screen.getAllByTestId("tencent-game")).toHaveLength(3);
    expect(screen.getByText(labels.tencent.noMore)).toBeTruthy();
  });

  test("a double-click opens the game and minimizes the window; a failure is explained", async () => {
    const { backend, onClose } = setup(["910001"]);
    await act(async () => fireEvent.click(screen.getByTestId("tencent-recent")));
    expect(backend.tencentSearch).toHaveBeenCalledWith("910001");
    await act(async () => fireEvent.doubleClick(screen.getAllByTestId("tencent-game")[1]!));
    expect(backend.tencentOpenGame).toHaveBeenCalledWith("810002");
    expect(onClose).toHaveBeenCalledTimes(1);

    backend.tencentOpenGame.mockReturnValueOnce(Promise.reject({ type: "Tencent", data: { type: "Request", data: { type: "Network" } } }));
    fireEvent.click(screen.getByTestId("tencent-after-stay"));
    await act(async () => fireEvent.click(screen.getAllByTestId("tencent-open")[0]!));
    expect(screen.getByTestId("tencent-open-error").textContent).toBe(labels.tencent.errors.Network);
    expect(onClose).toHaveBeenCalledTimes(1);

    await act(async () => fireEvent.click(screen.getByTestId("tencent-clear-recent")));
    expect(backend.tencentClearHistory).toHaveBeenCalled();
  });
});

describe("OpenUrlDialog", () => {
  test("a link is opened by Rust and the dialog closes; a refused one says why and stays", async () => {
    const openUrl = vi.fn().mockRejectedValueOnce({ type: "OpenUrl", data: { type: "UnsupportedYike" } }).mockResolvedValueOnce(3);
    const onClose = vi.fn();
    render(<OpenUrlDialog backend={{ openUrl }} labels={labels.url} onClose={onClose} />);
    const input = screen.getByTestId("url-input");
    expect(document.activeElement).toBe(input);
    fireEvent.change(input, { target: { value: " https://home.yikeweiqi.com/#/live/room/1 " } });
    await act(async () => fireEvent.click(screen.getByTestId("url-open")));
    expect(openUrl).toHaveBeenLastCalledWith("https://home.yikeweiqi.com/#/live/room/1");
    expect(screen.getByTestId("url-error").textContent).toBe(labels.url.errors.UnsupportedYike);
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.change(input, { target: { value: "https://example.org/game.sgf" } });
    await act(async () => fireEvent.click(screen.getByTestId("url-open")));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
