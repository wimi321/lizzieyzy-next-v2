import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import type { WebClientMessage } from "../api/generated/WebClientMessage";
import type { WebServerMessage } from "../api/generated/WebServerMessage";
import { backoff, INITIAL, reduce, socketUrl } from "./protocol";

export type Connection = "connecting" | "open" | "reconnecting";

/** The parts of the browser's WebSocket the page uses; tests pass a fake. */
export interface SocketLike {
  onopen: ((event: Event) => void) | null;
  onclose: ((event: CloseEvent) => void) | null;
  onmessage: ((event: MessageEvent) => void) | null;
  onerror: ((event: Event) => void) | null;
  readonly readyState: number;
  send(data: string): void;
  close(): void;
}

export type SocketFactory = (url: string) => SocketLike;

const OPEN = 1;
const browserSocket: SocketFactory = (url) => new WebSocket(url);

/**
 * One connection to the host, reopened after 1 s, 2 s … 30 s when it drops. The server's
 * reconnect credential travels with every reopen, so a trial survives a short drop.
 */
export function useWebSocketBoard(location: Pick<Location, "protocol" | "host" | "pathname">, open: SocketFactory = browserSocket) {
  const [state, dispatch] = useReducer(reduce, INITIAL);
  const [connection, setConnection] = useState<Connection>("connecting");
  const socket = useRef<SocketLike | null>(null);
  const credential = useRef<string | null>(null);
  const { protocol, host, pathname } = location;

  useEffect(() => {
    let stopped = false;
    let attempt = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const connect = () => {
      const next = open(socketUrl({ protocol, host, pathname }, credential.current));
      socket.current = next;
      next.onopen = () => {
        attempt = 0;
        setConnection("open");
      };
      next.onmessage = (event) => {
        let message: WebServerMessage;
        try {
          message = JSON.parse(String(event.data)) as WebServerMessage;
        } catch {
          return;
        }
        if (message.type === "Hello" && message.data.reconnect) credential.current = message.data.reconnect;
        dispatch(message);
      };
      next.onclose = () => {
        if (stopped || socket.current !== next) return;
        setConnection("reconnecting");
        timer = setTimeout(connect, backoff(attempt));
        attempt += 1;
      };
      next.onerror = () => undefined;
    };
    connect();
    return () => {
      stopped = true;
      clearTimeout(timer);
      socket.current?.close();
      socket.current = null;
    };
  }, [open, protocol, host, pathname]);

  const send = useCallback((message: WebClientMessage) => {
    const current = socket.current;
    if (current && current.readyState === OPEN) current.send(JSON.stringify(message));
  }, []);

  return { state, connection, send };
}
