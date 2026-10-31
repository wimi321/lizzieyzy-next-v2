import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { BoardSyncStatus } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { BoardSyncChip } from "./BoardSyncChip";

afterEach(cleanup);

function status(extra: Partial<BoardSyncStatus>): BoardSyncStatus {
  return { phase: "Syncing", always_sync: true, always_goto_last: false, remote_move_number: null, error: null, ...extra };
}

test("while syncing the chip shows the move readboard last read and stops it", () => {
  const onStop = vi.fn();
  render(<BoardSyncChip status={status({ remote_move_number: 87 })} labels={zh.sync} onStop={onStop} />);
  expect(screen.getByTestId("sync-chip").textContent).toContain("棋盘同步中");
  expect(screen.getByTestId("sync-move").textContent).toBe("第 87 手");
  fireEvent.click(screen.getByTestId("sync-stop"));
  expect(onStop).toHaveBeenCalledTimes(1);
});

test("starting and ready say so; stopped shows nothing", () => {
  const { rerender } = render(<BoardSyncChip status={status({ phase: "Starting" })} labels={zh.sync} onStop={vi.fn()} />);
  expect(screen.getByTestId("sync-chip").textContent).toContain("正在启动棋盘同步工具…");
  rerender(<BoardSyncChip status={status({ phase: "Ready" })} labels={zh.sync} onStop={vi.fn()} />);
  expect(screen.getByTestId("sync-chip").textContent).toContain("同步工具已就绪");
  rerender(<BoardSyncChip status={status({ phase: "Stopped" })} labels={zh.sync} onStop={vi.fn()} />);
  expect(screen.queryByTestId("sync-chip")).toBeNull();
});

test("a failure keeps the reason until it is closed", () => {
  const onStop = vi.fn();
  render(
    <BoardSyncChip
      status={status({ phase: "Failed", error: { type: "MissingExecutable", data: { path: "C:\\LizzieYzy\\readboard\\readboard.exe" } } })}
      labels={zh.sync}
      onStop={onStop}
    />,
  );
  const chip = screen.getByTestId("sync-chip");
  expect(chip.textContent).toContain("棋盘同步出错");
  expect(chip.getAttribute("title")).toBe("没有找到棋盘同步工具：C:\\LizzieYzy\\readboard\\readboard.exe");
  expect(screen.getByTestId("sync-stop").textContent).toBe("关闭");
  fireEvent.click(screen.getByTestId("sync-stop"));
  expect(onStop).toHaveBeenCalledTimes(1);
});
