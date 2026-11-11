import { useEffect, useState } from "react";
import type { Backend, EngineDisplayName, EngineProfile } from "@/api/backend";

function report(error: unknown) {
  console.error("Engine display name failed", error);
}

/**
 * The engine's name as LizzieYzy Next words it. Rust decides it; the three catalog tiers come
 * back as ids and are named here in the interface language. Null until Rust has answered.
 */
export function useEngineName(
  backend: Pick<Backend, "engineDisplayName">,
  profile: EngineProfile | null,
  catalog: Readonly<Record<string, string>>,
): string | null {
  const [named, setNamed] = useState<{ profile: EngineProfile; name: EngineDisplayName } | null>(null);
  useEffect(() => {
    if (!profile) return;
    let active = true;
    backend
      .engineDisplayName(profile)
      .then((name) => {
        if (active) setNamed({ profile, name });
      })
      .catch(report);
    return () => {
      active = false;
    };
  }, [backend, profile]);
  if (!profile || named?.profile !== profile) return null;
  return named.name.type === "Text" ? named.name.data : (catalog[named.name.data] ?? named.name.data);
}
