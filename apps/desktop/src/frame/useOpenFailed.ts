import { useEffect, useState } from "react";
import type { Backend, OpenFailed } from "@/api/backend";

function report(error: unknown) {
  console.error("Open-failed subscription failed", error);
}

/** The last game Rust could not open, pick or take from a drop, until the message is closed. */
export function useOpenFailed(backend: Pick<Backend, "subscribeOpenFailed" | "unsubscribe">): [OpenFailed | null, () => void] {
  const [failure, setFailure] = useState<OpenFailed | null>(null);
  useEffect(() => {
    let active = true;
    let id: number | undefined;
    backend
      .subscribeOpenFailed(setFailure)
      .then((subscription) => {
        if (active) id = subscription;
        else backend.unsubscribe(subscription).catch(report);
      })
      .catch(report);
    return () => {
      active = false;
      if (id !== undefined) backend.unsubscribe(id).catch(report);
    };
  }, [backend]);
  return [failure, () => setFailure(null)];
}
