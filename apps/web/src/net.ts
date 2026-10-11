import { PROTOCOL_VERSION, type ClientMsg, type ServerMsg } from "./protocol";

const HEADERS = { "content-type": "application/json", "x-hollowrite": "1" };

async function post(path: string, body: unknown): Promise<{ ok: boolean; data: Record<string, unknown> }> {
  const res = await fetch(path, { method: "POST", headers: HEADERS, body: JSON.stringify(body), credentials: "same-origin" });
  const data: Record<string, unknown> = await res.json().catch(() => ({}));
  return { ok: res.ok, data };
}

export async function authenticate(username: string, password: string, register: boolean): Promise<string | null> {
  const { ok, data } = await post(register ? "/api/register" : "/api/login", { username, password, client: "web" });
  return ok ? null : String(data.error ?? "something went wrong");
}

export async function currentUser(): Promise<string | null> {
  const res = await fetch("/api/me", { credentials: "same-origin" });
  if (!res.ok) return null;
  const data = await res.json();
  return typeof data.username === "string" ? data.username : null;
}

export async function logout(): Promise<void> {
  await post("/api/logout", {});
}

export interface Connection {
  send(msg: ClientMsg): void;
  close(): void;
}

/** Opens the game socket. The session cookie authenticates it; the server
 * checks the Origin header, so no token ever lives in script-visible storage. */
export function connect(
  onMessage: (msg: ServerMsg) => void,
  onRtt: (ms: number) => void,
  onClose: (reason: string) => void,
): Connection {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  const ws = new WebSocket(`${scheme}://${location.host}/ws`);
  let pingTimer = 0;
  let nonce = 0;
  let pending: { nonce: number; at: number } | null = null;
  let closed = false;

  const send = (msg: ClientMsg) => {
    if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(msg));
  };

  ws.onopen = () => {
    send({ t: "hello", protocol: PROTOCOL_VERSION, client: "web", build: "web" });
    pingTimer = window.setInterval(() => {
      nonce = (nonce + 1) >>> 0;
      pending = { nonce, at: performance.now() };
      send({ t: "ping", nonce });
    }, 2000);
  };
  ws.onmessage = (ev) => {
    let msg: ServerMsg;
    try {
      msg = JSON.parse(String(ev.data));
    } catch {
      return;
    }
    if (msg.t === "pong") {
      if (pending && pending.nonce === msg.nonce) onRtt(performance.now() - pending.at);
      return;
    }
    onMessage(msg);
  };
  ws.onclose = (ev) => {
    window.clearInterval(pingTimer);
    if (!closed) onClose(ev.reason || (ev.code === 1006 ? "connection lost" : "disconnected"));
    closed = true;
  };
  return {
    send,
    close() {
      closed = true;
      window.clearInterval(pingTimer);
      ws.close();
    },
  };
}
