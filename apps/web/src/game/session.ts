import { connect, type Connection } from "../net";
import { cleanText, type CharacterSummary, type ClientMsg, type ServerMsg } from "../protocol";
import { Model } from "./model";

export type Phase = "connecting" | "lobby" | "game" | "closed";

/** Owns the socket and the client model; React subscribes for re-renders. */
export class Session {
  phase: Phase = "connecting";
  account = "";
  characters: CharacterSummary[] | null = null;
  lobbyError: string | null = null;
  closedReason: string | null = null;
  kicked: string | null = null;
  model: Model | null = null;
  private conn: Connection;
  private welcome = { walkSpeed: 120, bodyRadius: 300, tickHz: 30 };
  private listeners = new Set<() => void>();
  private version = 0;
  rtt: number | null = null;

  constructor() {
    this.conn = connect(
      (m) => this.handle(m),
      (ms) => {
        this.rtt = this.rtt === null ? ms : (this.rtt * 3 + ms) / 4;
        if (this.model) this.model.rtt = this.rtt;
      },
      (reason) => {
        this.phase = "closed";
        this.closedReason = cleanText(reason);
        this.bump();
      },
    );
  }

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  };

  snapshot = () => this.version;

  bump() {
    this.version++;
    for (const l of this.listeners) l();
  }

  send(msg: ClientMsg) {
    this.conn.send(msg);
  }

  close() {
    this.conn.close();
  }

  private handle(msg: ServerMsg) {
    switch (msg.t) {
      case "welcome":
        this.account = msg.account;
        this.welcome = { walkSpeed: msg.walk_speed || 120, bodyRadius: msg.body_radius || 300, tickHz: msg.tick_hz || 30 };
        this.phase = "lobby";
        this.send({ t: "list_characters" });
        break;
      case "characters":
        this.characters = msg.list;
        this.phase = "lobby";
        this.model = null;
        this.lobbyError = null;
        break;
      case "error":
        if (this.phase === "game" && this.model) this.model.toast(cleanText(msg.message));
        else this.lobbyError = cleanText(msg.message);
        break;
      case "enter_world":
        this.model = new Model(msg.you, msg.map, msg.sheet, this.welcome.walkSpeed, this.welcome.bodyRadius, this.welcome.tickHz);
        this.model.rtt = this.rtt;
        this.phase = "game";
        break;
      case "snapshot":
        this.model?.applySnapshot(msg);
        return;
      case "sheet":
        if (this.model) this.model.sheet = msg.sheet;
        break;
      case "events":
        this.model?.applyEvents(msg.events);
        break;
      case "chat":
        this.model?.pushChat(msg.channel, msg.from, msg.text);
        break;
      case "party":
        if (this.model) {
          this.model.party = msg.members;
          this.model.invites = msg.invites;
        }
        break;
      case "dialogue":
        if (this.model) {
          this.model.dialogue = { npc: msg.npc, speaker: cleanText(msg.speaker), lines: msg.lines.map(cleanText), choices: msg.choices.map(cleanText) };
        }
        break;
      case "kicked":
        this.kicked = cleanText(msg.reason);
        break;
      case "pong":
        return;
    }
    this.bump();
  }
}
