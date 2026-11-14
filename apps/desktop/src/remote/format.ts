import type { EngineProfile } from "@/api/backend";
import { reasonOf } from "../everyday/useEveryday";
import type en from "../locales/en.json";

type Labels = typeof en.remote;
export type RemoteKind = "Zhizi" | "WebSocket" | "Ssh";

export function remoteKind(profile: EngineProfile | null | undefined): RemoteKind | null {
  return profile?.remote?.type ?? null;
}

/** "智子云 · VIP 包月 · Transformer 10B", "自建算力 · gpu.example.org:443", "SSH · go@10.0.0.2:22". */
export function remoteSummary(profile: EngineProfile | null | undefined, labels: Labels): string | null {
  const remote = profile?.remote;
  if (!remote) return null;
  switch (remote.type) {
    case "Zhizi":
      return [labels.kinds.Zhizi, labels.zhizi.plans[remote.data.options?.plan ?? "Vip"], labels.zhizi.weights[remote.data.options?.weight ?? "10b512t"]].join(" · ");
    case "WebSocket":
      return `${labels.kinds.WebSocket} · ${remote.data.host ?? ""}:${remote.data.port ?? ""}`;
    case "Ssh":
      return `${labels.kinds.Ssh} · ${remote.data.user ?? ""}@${remote.data.host ?? ""}:${remote.data.port ?? 22}`;
  }
}

type Typed = { type: string; data?: unknown };

function typed(value: unknown): Typed | null {
  return typeof value === "object" && value !== null && "type" in value ? (value as Typed) : null;
}

/**
 * Remote engine, link and 智子 failures in the interface language. A changed host key names both
 * fingerprints and the file, as Rust reports it; a 智子 failure adds its request id.
 */
export function remoteError(error: unknown, labels: Labels): string {
  const outer = typed(error);
  if (outer?.type === "RemoteLink" && typeof outer.data === "string") {
    return (labels.linkErrors as Record<string, string>)[outer.data] ?? outer.data;
  }
  if (outer?.type === "Zhizi" && typeof outer.data === "object" && outer.data !== null) {
    const failure = outer.data as { kind?: Typed; request_id?: string };
    const kind = failure.kind?.type ?? "Unknown";
    const text = (labels.zhiziErrors as Record<string, string>)[kind] ?? labels.zhiziErrors.Unknown;
    return failure.request_id ? `${text} ${labels.requestId.replace("{id}", failure.request_id)}` : text;
  }
  const inner = outer?.type === "Remote" ? typed(outer.data) : null;
  if (!inner) return reasonOf(error);
  if (inner.type === "KnownHosts") {
    return labels.engineErrors.KnownHosts.replace("{path}", (inner.data as { path: string }).path);
  }
  return (labels.engineErrors as Record<string, string>)[inner.type] ?? reasonOf(error);
}

/** The fingerprints of a changed SSH host key, shown beside the message. */
export function changedHostKey(error: unknown): { old_fingerprint: string; new_fingerprint: string; path: string } | null {
  const outer = typed(error);
  const inner = outer?.type === "Remote" ? typed(outer.data) : null;
  return inner?.type === "HostKeyChanged" ? (inner.data as { old_fingerprint: string; new_fingerprint: string; path: string }) : null;
}

/** "¥12.50": 智子 sends yuan as decimal text. */
export function yuan(value: string | undefined): string {
  return value === undefined || value === "" ? "--" : `¥${value}`;
}

/** Java's "{h} 小时 {m} 分钟" / "{m} 分钟". */
export function duration(seconds: number | undefined, labels: Labels): string {
  const minutes = Math.round((seconds ?? 0) / 60);
  const hours = Math.floor(minutes / 60);
  return hours > 0
    ? labels.zhizi.hoursMinutes.replace("{h}", String(hours)).replace("{m}", String(minutes % 60))
    : labels.zhizi.minutes.replace("{m}", String(minutes));
}

/** "2026-11-14T09:30:12+08:00" → "2026-11-14 09:30". */
export function shortTime(value: string | null | undefined): string {
  if (!value) return "--";
  const match = /^(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2})/.exec(value);
  return match ? `${match[1]} ${match[2]}` : value;
}
