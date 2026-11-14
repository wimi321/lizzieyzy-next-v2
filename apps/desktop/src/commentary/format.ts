import type { CommentaryPhase, CommentaryUpdated } from "../api/commentary";
import { reasonOf } from "../everyday/useEveryday";
import type en from "../locales/en.json";

type Labels = typeof en.commentary;

type Typed = { type: string; data?: unknown };

function typed(value: unknown): Typed | null {
  return typeof value === "object" && value !== null && "type" in value ? (value as Typed) : null;
}

/** A `CommentaryError`, bare or inside `CommandError::Commentary`, in the interface language. */
export function commentaryError(error: unknown, labels: Labels): string {
  let failure = typed(error);
  if (failure?.type === "Commentary") failure = typed(failure.data);
  if (!failure) return reasonOf(error);
  if (failure.type === "Client") {
    const client = typed(failure.data);
    if (!client) return reasonOf(error);
    if (client.type === "Http") {
      const data = client.data as { status: number; body: string };
      return labels.client.Http.replace("{status}", String(data.status)).replace("{body}", data.body);
    }
    return (labels.client as Record<string, string>)[client.type] ?? reasonOf(error);
  }
  if (failure.type === "InvalidSettings") return labels.errors.InvalidSettings.replace("{field}", String(failure.data ?? ""));
  return (labels.errors as Record<string, string>)[failure.type] ?? reasonOf(error);
}

const WORKING: readonly CommentaryPhase[] = ["Preparing", "Generating", "Verifying"];

export function isWorking(view: CommentaryUpdated | undefined): boolean {
  return view?.phase !== undefined && WORKING.includes(view.phase);
}

/** The status line under the answer: the phase, what was selected, and why it failed. */
export function statusText(view: CommentaryUpdated, writing: boolean, labels: Labels): string {
  if (view.error) return labels.failed.replace("{reason}", commentaryError(view.error, labels));
  const phase = view.phase ?? "Stopped";
  if (phase === "Completed" && writing && (view.issues ?? []).length === 0) return labels.saved;
  if (phase === "Failed" && (view.issues ?? []).length > 0) return labels.unverified;
  const selected = view.selected ?? 0;
  if (phase === "Generating" && selected > 0) {
    return labels.selected
      .replace("{selected}", String(selected))
      .replace("{analyzed}", String(view.analyzed ?? 0))
      .replace("{omitted}", String(view.omitted ?? 0));
  }
  return labels.phases[phase];
}
