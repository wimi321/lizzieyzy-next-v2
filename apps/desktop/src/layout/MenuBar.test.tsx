import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { type MenuDefinition, MenuBar } from "./MenuBar";

afterEach(cleanup);

function menus(open: () => void, toggle: () => void): MenuDefinition[] {
  return [
    {
      label: "文件",
      testId: "menu-file",
      items: [
        { kind: "action", label: "打开…", testId: "menu-open", onSelect: open },
        { kind: "action", label: "保存…", testId: "menu-save", onSelect: vi.fn(), disabled: true },
      ],
    },
    { label: "显示", testId: "menu-view", items: [{ kind: "check", label: "坐标", testId: "menu-coordinates", checked: true, onSelect: toggle }] },
  ];
}

test("a menu opens on click, runs the chosen item and closes", () => {
  const open = vi.fn();
  render(<MenuBar menus={menus(open, vi.fn())} />);
  expect(screen.queryByRole("menu")).toBeNull();

  fireEvent.click(screen.getByTestId("menu-file"));
  expect(screen.getByRole("menu")).toBeTruthy();
  expect((screen.getByTestId("menu-save") as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByTestId("menu-open"));
  expect(open).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole("menu")).toBeNull();
});

test("while one menu is open, hovering another switches to it; Escape and outside clicks close", () => {
  const toggle = vi.fn();
  render(
    <>
      <MenuBar menus={menus(vi.fn(), toggle)} />
      <p>outside</p>
    </>,
  );
  fireEvent.mouseEnter(screen.getByTestId("menu-view"));
  expect(screen.queryByRole("menu")).toBeNull();

  fireEvent.click(screen.getByTestId("menu-file"));
  fireEvent.mouseEnter(screen.getByTestId("menu-view"));
  expect(screen.getByTestId("menu-coordinates").getAttribute("aria-checked")).toBe("true");

  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();

  fireEvent.click(screen.getByTestId("menu-view"));
  fireEvent.pointerDown(screen.getByText("outside"));
  expect(screen.queryByRole("menu")).toBeNull();
  expect(toggle).not.toHaveBeenCalled();
});
