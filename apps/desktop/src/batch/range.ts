import type { WholeGameScope } from "@/api/backend";

export function wholeNumber(text: string): number | null {
  const trimmed = text.trim();
  return /^\d+$/.test(trimmed) ? Number(trimmed) : null;
}

/** An optional move number: blank is "the first" or "the last". */
function moveNumber(text: string): number | null | undefined {
  if (text.trim() === "") return null;
  const value = wholeNumber(text);
  return value !== null && value >= 1 ? value : undefined;
}

export function rangeScope(first: string, last: string): WholeGameScope | null {
  const start = moveNumber(first);
  const end = moveNumber(last);
  if (start === undefined || end === undefined) return null;
  if (start !== null && end !== null && start > end) return null;
  return { type: "Range", data: { first_move: start, last_move: end } };
}
