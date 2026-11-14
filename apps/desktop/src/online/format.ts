import type { FoxFailure } from "@/api/backend";
import type { TencentPlayer, TencentRank } from "../api/tencent";
import { reasonOf } from "../everyday/useEveryday";

export interface RankLabels {
  dan: string;
  kyu: string;
  pro: string;
}

/** Rust converts Tencent's raw grade; Java writes it "5段", "3级" or "P2段". */
export function rankText(rank: TencentRank | undefined, labels: RankLabels): string {
  if (!rank) return "";
  const template = rank.type === "Dan" ? labels.dan : rank.type === "Kyu" ? labels.kyu : labels.pro;
  return template.replace("{n}", String(rank.data));
}

/** Tencent gives Unix seconds; the list shows them in the computer's own time, to the minute. */
export function startTime(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return "";
  const date = new Date(seconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** Java's "昵称 (UID)", with only what Tencent sent. */
export function playerText(player: TencentPlayer | undefined): string {
  const name = player?.nickname ?? "";
  const uid = player?.uid ?? "";
  return name && uid ? `${name} (${uid})` : name || uid;
}

/** Whether the searched text names this player, by nickname or UID. */
export function isQueried(player: TencentPlayer | undefined, query: string): boolean {
  return query !== "" && (player?.uid === query || player?.nickname === query);
}

export interface RequestLabels {
  Timeout: string;
  Network: string;
  Http: string;
  InvalidResponse: string;
  ResponseTooLarge: string;
  Request: string;
}

function requestText(failure: FoxFailure, labels: RequestLabels): string {
  switch (failure.type) {
    case "Timeout":
    case "Network":
    case "InvalidResponse":
    case "ResponseTooLarge":
      return labels[failure.type];
    case "Http":
      return labels.Http.replace("{status}", String(failure.data.status));
    default:
      return labels.Request;
  }
}

/** The typed failure inside `CommandError::<kind>`, or null for anything else. */
function inner(error: unknown, kind: string): { type: string; data?: unknown } | null {
  if (typeof error !== "object" || error === null || !("type" in error)) return null;
  const outer = error as { type: string; data?: unknown };
  if (outer.type !== kind || typeof outer.data !== "object" || outer.data === null || !("type" in outer.data)) return null;
  return outer.data as { type: string; data?: unknown };
}

export interface TencentErrorLabels extends RequestLabels {
  InvalidQuery: string;
  Api: string;
}

export function tencentError(error: unknown, labels: TencentErrorLabels): string {
  const failure = inner(error, "Tencent");
  if (!failure) return reasonOf(error);
  if (failure.type === "InvalidQuery") return labels.InvalidQuery;
  if (failure.type === "Api") {
    const data = failure.data as { result: number; message: string };
    return labels.Api.replace("{message}", data.message || String(data.result));
  }
  if (failure.type === "Request") return requestText(failure.data as FoxFailure, labels);
  return reasonOf(error);
}

export interface UrlErrorLabels extends RequestLabels {
  InvalidUrl: string;
  UnsupportedScheme: string;
  UnsupportedYike: string;
  RedirectLimit: string;
  InvalidRedirect: string;
  InvalidSgf: string;
}

export function urlError(error: unknown, labels: UrlErrorLabels): string {
  const failure = inner(error, "OpenUrl");
  if (!failure) return reasonOf(error);
  if (failure.type === "Request") return requestText(failure.data as FoxFailure, labels);
  const known = (labels as unknown as Record<string, string>)[failure.type];
  return known ?? reasonOf(error);
}
