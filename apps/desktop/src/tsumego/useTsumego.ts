import { useEffect, useState } from "react";
import type { Backend, TsumegoOptions, TsumegoStatus } from "@/api/backend";

/** Java's starting 死活题 options, until the settings say otherwise. */
const DEFAULTS: TsumegoOptions = { to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1 };

function report(error: unknown) {
  console.error("Tsumego command failed", error);
}

/** The 死活题 dialog, its last options, and the framed problem while one is on the board. */
export function useTsumego(backend: Pick<Backend, "generateTsumego" | "tsumegoStatus" | "exitTsumego">, revision: number) {
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<TsumegoOptions>(DEFAULTS);
  // Set while a framed problem is on the board; null otherwise.
  const [status, setStatus] = useState<TsumegoStatus | null>(null);

  // Rust ends a problem when another game is opened; while one is on, re-read its status.
  const framing = status !== null;
  useEffect(() => {
    if (!framing) return;
    let active = true;
    backend
      .tsumegoStatus()
      .then((next) => {
        if (active && next.revision >= revision) setStatus(next.active ? next : null);
      })
      .catch(report);
    return () => {
      active = false;
    };
  }, [backend, framing, revision]);

  const generate = async (chosen: TsumegoOptions) => {
    await backend.generateTsumego(chosen);
    setOptions(chosen);
    setOpen(false);
    const next = await backend.tsumegoStatus();
    setStatus(next.active ? next : null);
  };
  const exit = () => {
    backend
      .exitTsumego()
      .then(() => setStatus(null))
      .catch(report);
  };
  return { open, setOpen, options, setOptions, status, generate, exit };
}
