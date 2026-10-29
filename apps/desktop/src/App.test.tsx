import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import App from "./App";
import { getMessages } from "./locales";

afterEach(cleanup);

test("renders only the application title on the home page", () => {
  render(<App />);
  const heading = screen.getByRole("heading", { level: 1 });
  expect(heading.textContent).toBe("LizzieYzy Next 2 (dev)");
  expect(screen.getByRole("main").textContent).toBe(heading.textContent);
});

test.each(["zh-CN", "en", "fr"])("provides the application title for %s", (language) => {
  expect(getMessages(language).app.title).toBe("LizzieYzy Next 2 (dev)");
});
