import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useEffect } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { BoardView, Settings } from "@/api/backend";
import type { CommentaryKeyStatus, CommentarySettings, CommentaryUpdated } from "../api/commentary";
import en from "../locales/en.json";
import { NotesPanel } from "../panels/NotesPanel";
import { splitComment } from "./block";
import { parseMarkdown } from "./blocks";
import { commentaryError } from "./format";
import { Markdown } from "./Markdown";
import { useCommentary } from "./useCommentary";

afterEach(cleanup);
const labels = en.commentary;

test("the Markdown subset becomes blocks; nothing in a reply can become HTML", () => {
  expect(parseMarkdown("## Plan\n\n- **D4** first\n- then `Q16`\n\n1. one\n2. two\n\n> quoted\n\n```\nA B\n")).toEqual([
    { kind: "heading", level: 2, inlines: [{ kind: "text", text: "Plan" }] },
    { kind: "list", ordered: false, items: [[{ kind: "strong", text: "D4" }, { kind: "text", text: " first" }], [{ kind: "text", text: "then " }, { kind: "code", text: "Q16" }]] },
    { kind: "list", ordered: true, items: [[{ kind: "text", text: "one" }], [{ kind: "text", text: "two" }]] },
    { kind: "quote", inlines: [{ kind: "text", text: "quoted" }] },
    { kind: "code", text: "A B\n" },
  ]);
  const { container } = render(<Markdown text={'<img src=x onerror="alert(1)"> [link](https://example.org) *calm*'} />);
  expect(container.querySelector("img, a, script")).toBeNull();
  expect(container.querySelector("em")?.textContent).toBe("calm");
  expect(container.textContent).toContain('<img src=x onerror="alert(1)">');
});

const BLOCK = (body: string) => `[LizzieYzy AI Commentary BEGIN]\ngeneratedAt=2026-11-14T09:30:12Z\nmodel=demo-model\n\n${body}\n[LizzieYzy AI Commentary END]`;

test("the comment splits into the user's text and Rust's last AI block, follow-ups after it", () => {
  const comment = `My note\n\n${BLOCK("old")}\n\n${BLOCK("Black should **extend**.\n\n追问：Why?\n\nBecause of the cut.")}`;
  expect(splitComment(comment)).toEqual({
    own: "My note",
    ai: {
      model: "demo-model",
      generatedAt: "2026-11-14T09:30:12Z",
      sections: [
        { question: null, body: "Black should **extend**." },
        { question: "Why?", body: "Because of the cut." },
      ],
    },
  });
  // An unfinished block is the user's own text, as in Rust's reader.
  expect(splitComment("note [LizzieYzy AI Commentary BEGIN] half")).toEqual({ own: "note [LizzieYzy AI Commentary BEGIN] half", ai: null });

  render(
    <NotesPanel tab="comment" onTab={() => {}} problemCount={0} status={null} summary={[]} comment={comment} labels={en.notes}>
      {null}
    </NotesPanel>,
  );
  expect(screen.getByTestId("comment-text").querySelector("p")?.textContent).toBe("My note");
  const ai = screen.getByTestId("comment-ai");
  expect(ai.querySelector("h3")?.textContent).toBe("AI Commentarydemo-model · 2026-11-14 09:30");
  expect(ai.querySelector("strong")?.textContent).toBe("extend");
  expect(ai.querySelector("h4")?.textContent).toBe("Follow-up: Why?");
  expect(ai.textContent).not.toContain("LizzieYzy AI Commentary");
});

test("commentary failures read in the interface language", () => {
  expect(commentaryError({ type: "Commentary", data: { type: "NoAnalysis" } }, labels)).toBe(labels.errors.NoAnalysis);
  expect(commentaryError({ type: "Commentary", data: { type: "InvalidSettings", data: "rank" } }, labels)).toBe("A setting is not valid: rank");
  expect(commentaryError({ type: "Commentary", data: { type: "Client", data: { type: "Http", data: { status: 401, body: "[REDACTED] denied" } } } }, labels)).toBe(
    "The AI service returned an error (HTTP 401): [REDACTED] denied",
  );
  expect(commentaryError({ type: "Client", data: { type: "RateLimited", data: { retry_after: "30", body: "" } } }, labels)).toBe(labels.client.RateLimited);
});

describe("useCommentary", () => {
  const board = { move_number: 42 } as BoardView;
  function harness(settings: CommentarySettings, key: CommentaryKeyStatus) {
    let listener: ((view: CommentaryUpdated) => void) | undefined;
    let keyState = key;
    const backend = {
      startCommentary: vi.fn(() => Promise.resolve<CommentaryUpdated>({ request_id: 1, phase: "Preparing" })),
      stopCommentary: vi.fn(() => Promise.resolve<CommentaryUpdated>({ request_id: 1, phase: "Stopped" })),
      commentaryModels: vi.fn(() => Promise.resolve(["demo-model", "demo-large"])),
      setCommentaryKey: vi.fn(() => Promise.resolve((keyState = { has_key: true, stored: true, available: true }))),
      clearCommentaryKey: vi.fn(),
      commentaryKeyStatus: vi.fn(() => Promise.resolve(keyState)),
      setCommentarySettings: vi.fn((next: CommentarySettings) => Promise.resolve(next)),
      subscribeCommentaryUpdated: vi.fn((next: (view: CommentaryUpdated) => void) => ((listener = next), Promise.resolve(9))),
      unsubscribe: vi.fn(() => Promise.resolve()),
    };
    const jump = vi.fn();
    let hook: ReturnType<typeof useCommentary> | undefined;
    function Host() {
      const value = useCommentary(backend, labels, { board, jump });
      useEffect(() => {
        hook = value;
      });
      return <>{value.windows}</>;
    }
    render(<Host />);
    act(() => hook!.load({ commentary: settings } as Settings));
    const emit = (view: CommentaryUpdated) => act(() => listener!(view));
    return { backend, jump, emit, hook: () => hook! };
  }

  test("the answer streams in; a follow-up adds its own section; 停止 cancels", async () => {
    const { backend, emit, hook } = harness({ base_url: "https://ai.example.org/v1", model: "demo-model" }, { has_key: true, available: true });
    await act(async () => hook().bindings.aiCommentary!.run!());
    expect(screen.getByTestId("commentary-position").textContent).toBe("Move 42");
    await act(async () => fireEvent.click(screen.getByTestId("commentary-start")));
    expect(backend.startCommentary).toHaveBeenLastCalledWith({ mode: { type: "NextMove" }, follow_up: null });

    emit({ request_id: 1, phase: "Generating", text: "Black should **extend** at D4" });
    expect(screen.getByTestId("commentary-answer").querySelector("strong")?.textContent).toBe("extend");
    expect(screen.getByTestId("commentary-stop")).toBeTruthy();
    emit({ request_id: 1, phase: "Completed", text: "Black should **extend** at D4.", evidence: "Move 42, Black to play" });
    expect(screen.getByTestId("commentary-status").textContent).toBe(labels.saved + "Model: demo-model");
    fireEvent.click(screen.getByTestId("commentary-tab-evidence"));
    expect(screen.getByTestId("commentary-evidence").textContent).toBe("Move 42, Black to play");

    backend.startCommentary.mockReturnValueOnce(Promise.resolve({ request_id: 2, phase: "Preparing" }));
    await act(async () => fireEvent.click(screen.getByTestId("commentary-simpler")));
    expect(backend.startCommentary).toHaveBeenLastCalledWith({ mode: { type: "NextMove" }, follow_up: labels.simpler });
    emit({ request_id: 2, phase: "Generating", text: "Make the group strong first." });
    const answers = screen.getAllByTestId("commentary-answer");
    expect(answers).toHaveLength(2);
    expect(answers[1]!.querySelector("h3")?.textContent).toBe(`Follow-up: ${labels.simpler}`);
    await act(async () => fireEvent.click(screen.getByTestId("commentary-stop")));
    expect(backend.stopCommentary).toHaveBeenCalled();
  });

  test("a range is checked before sending, its key positions jump to their move", async () => {
    const { backend, jump, emit, hook } = harness({ base_url: "https://ai.example.org/v1", model: "demo-model" }, { has_key: true, available: true });
    await act(async () => hook().bindings.aiCommentary!.run!());
    fireEvent.click(screen.getByTestId("commentary-mode-range"));
    expect((screen.getByTestId("commentary-from") as HTMLInputElement).value).toBe("33");
    fireEvent.change(screen.getByTestId("commentary-from"), { target: { value: "50" } });
    await act(async () => fireEvent.click(screen.getByTestId("commentary-start")));
    expect(backend.startCommentary).not.toHaveBeenCalled();
    expect(screen.getByTestId("commentary-status").textContent).toContain(labels.errors.InvalidRange);

    fireEvent.change(screen.getByTestId("commentary-from"), { target: { value: "30" } });
    await act(async () => fireEvent.click(screen.getByTestId("commentary-start")));
    expect(backend.startCommentary).toHaveBeenLastCalledWith({ mode: { type: "Range", data: { from: 30, to: 42 } }, follow_up: null });
    emit({ request_id: 1, phase: "Generating", positions: [{ node_id: 31, move_number: 31 }, { node_id: 38, move_number: 38 }], selected: 2, analyzed: 12, omitted: 1 });
    expect(screen.getByTestId("commentary-status").textContent).toContain("2 key positions selected (12 analyzed, 1 left out)");
    fireEvent.click(screen.getAllByTestId("commentary-key-position")[1]!);
    expect(jump).toHaveBeenCalledWith(38);
  });

  test("without a service 连接 AI opens the settings; saving stores the settings, then the key", async () => {
    const { backend, hook } = harness({}, { has_key: false, available: true });
    await act(async () => hook().bindings.aiCommentary!.run!());
    expect(screen.getByTestId("commentary-start").textContent).toBe(labels.connect);
    fireEvent.click(screen.getByTestId("commentary-start"));
    expect(screen.getByTestId("commentary-privacy").textContent).toBe(labels.setup.privacy);

    await act(async () => fireEvent.click(screen.getByTestId("commentary-save")));
    expect(screen.getByTestId("commentary-setup-error").textContent).toBe(labels.setup.enterAddress);
    fireEvent.change(screen.getByTestId("commentary-url"), { target: { value: "https://ai.example.org/v1" } });
    fireEvent.change(screen.getByTestId("commentary-key"), { target: { value: "sk-test" } });
    fireEvent.click(screen.getByTestId("commentary-remember"));
    await act(async () => fireEvent.click(screen.getByTestId("commentary-refresh-models")));
    expect(backend.commentaryModels).toHaveBeenCalled();
    expect((screen.getByTestId("commentary-model") as HTMLInputElement).value).toBe("demo-model");
    expect(screen.getByTestId("commentary-models-note").textContent).toBe("2 models loaded.");
    fireEvent.change(screen.getByTestId("commentary-rank"), { target: { value: "2d" } });
    await act(async () => fireEvent.click(screen.getByTestId("commentary-save")));
    expect(backend.setCommentarySettings).toHaveBeenLastCalledWith(expect.objectContaining({ base_url: "https://ai.example.org/v1", model: "demo-model", rank: "2d", remember_key: true }));
    // The key was sent with the models request; saving again leaves it alone.
    expect(backend.setCommentaryKey).toHaveBeenCalledTimes(1);
    expect(backend.setCommentaryKey).toHaveBeenCalledWith("sk-test", true);
    expect(screen.queryByTestId("commentary-setup")).toBeNull();
    expect(screen.getByTestId("commentary-start").textContent).toBe(labels.start);
  });
});
