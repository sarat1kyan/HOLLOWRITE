import { Fragment, useEffect, useRef, useState, type FormEvent } from "react";
import { ATTRIBUTES, CELL, MAX_CHAT_CHARS, dirFromOffset, type ClientMsg, type EquipSlot } from "../protocol";
import type { Session } from "../game/session";
import { Input } from "../game/input";
import { Renderer } from "../game/render";
import { shade, uiColor } from "../game/theme";
import type { Settings } from "../settings";
import { MapView } from "./MapView";

type Overlay = "none" | "character" | "level" | "map" | "help" | "menu";

function Bar({ label, cur, max, fg, back }: { label: string; cur: number; max: number; fg: string; back: string }) {
  const pct = max > 0 ? Math.min(100, (cur / max) * 100) : 0;
  return (
    <div className="bar" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={max} aria-valuenow={cur}>
      <span className="label" style={{ color: shade(fg, 1) }}>{label}</span>
      <span className="track" style={{ background: shade(back, 1) }}>
        <span className="fill" style={{ width: `${pct}%`, background: shade(fg, 1), display: "block" }} />
      </span>
      <span className="num">{cur}/{max}</span>
    </div>
  );
}

function parseChat(text: string): ClientMsg | "help" | null {
  const t = text.trim();
  if (!t) return null;
  const sp = t.indexOf(" ");
  const cmd = sp < 0 ? t : t.slice(0, sp);
  const rest = sp < 0 ? "" : t.slice(sp + 1).trim();
  switch (cmd) {
    case "/p":
    case "/party": return { t: "chat", channel: "party", text: rest };
    case "/invite": return { t: "party_invite", name: rest };
    case "/accept": return { t: "party_accept", from: rest };
    case "/leave": return { t: "party_leave" };
    case "/mute": return { t: "mute", name: rest, on: true };
    case "/unmute": return { t: "mute", name: rest, on: false };
    case "/report": {
      const i = rest.indexOf(" ");
      return { t: "report", name: i < 0 ? rest : rest.slice(0, i), reason: i < 0 ? "" : rest.slice(i + 1) };
    }
    default:
      return cmd.startsWith("/") ? "help" : { t: "chat", channel: "local", text: t };
  }
}

export default function GameScreen({ session, settings, setSettings }: {
  session: Session;
  settings: Settings;
  setSettings: (s: Settings) => void;
}) {
  const model = session.model!;
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const chatRef = useRef<HTMLInputElement>(null);
  const logRef = useRef<HTMLUListElement>(null);
  const rendererRef = useRef<Renderer | null>(null);
  const inputRef = useRef<Input | null>(null);
  const [overlay, setOverlay] = useState<Overlay>("none");
  const [chat, setChat] = useState("");
  const [sel, setSel] = useState(0);
  const [, setTick] = useState(0);
  const [narrow, setNarrow] = useState(() => window.innerWidth < 900);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const overlayRef = useRef(overlay);
  overlayRef.current = overlay;
  const dialogueOpen = !!model.dialogue;

  useEffect(() => {
    const id = window.setInterval(() => setTick((t) => t + 1), 100);
    const onResize = () => setNarrow(window.innerWidth < 900);
    window.addEventListener("resize", onResize);
    return () => {
      window.clearInterval(id);
      window.removeEventListener("resize", onResize);
    };
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current!;
    const renderer = new Renderer(canvas, { ...settingsRef.current });
    rendererRef.current = renderer;
    const input = new Input((m) => session.send(m));
    inputRef.current = input;
    renderer.resize();
    const ro = new ResizeObserver(() => renderer.resize());
    ro.observe(canvas);
    let raf = 0;
    let last = performance.now();
    const frame = (now: number) => {
      const m = session.model;
      if (m) {
        const typing = document.activeElement === chatRef.current || overlayRef.current !== "none" || !!m.dialogue;
        if (!typing) input.tick(now);
        m.predict(now, now - last, typing ? null : input.dir());
        const cell = m.myCell(now);
        if (cell) m.updateFov(cell);
        m.prune(now);
        renderer.draw(m, now);
      }
      last = now;
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    canvas.focus();
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [session]);

  useEffect(() => {
    if (rendererRef.current) {
      rendererRef.current.opts = { ...settings };
      rendererRef.current.resize();
    }
  }, [settings]);

  useEffect(() => {
    const onDown = (e: KeyboardEvent) => {
      const input = inputRef.current;
      if (!input) return;
      const inChat = document.activeElement === chatRef.current;
      if (e.key === "Escape") {
        if (inChat) chatRef.current?.blur();
        else if (session.model?.dialogue) session.model.dialogue = null;
        else setOverlay((o) => (o === "none" ? "menu" : "none"));
        canvasRef.current?.focus();
        e.preventDefault();
        return;
      }
      if (inChat || overlayRef.current !== "none" || session.model?.dialogue) return;
      if (input.keydown(e)) {
        e.preventDefault();
        return;
      }
      const open = (o: Overlay) => {
        input.release();
        setSel(0);
        setOverlay(o);
        e.preventDefault();
      };
      if (e.code === "Tab") open("character");
      else if (e.code === "KeyM") open("map");
      else if (e.code === "KeyV") open("level");
      else if (e.key === "?" || e.code === "F1") open("help");
      else if (e.code === "Enter" || e.code === "KeyT" || e.code === "Slash") {
        input.release();
        e.preventDefault();
        if (e.code === "Slash") setChat("/");
        chatRef.current?.focus();
      }
    };
    const onUp = (e: KeyboardEvent) => {
      if (inputRef.current?.keyup(e)) e.preventDefault();
    };
    const onBlur = () => inputRef.current?.release();
    window.addEventListener("keydown", onDown);
    window.addEventListener("keyup", onUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onDown);
      window.removeEventListener("keyup", onUp);
      window.removeEventListener("blur", onBlur);
    };
  }, [session]);

  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [model.chat.length]);

  const submitChat = (e: FormEvent) => {
    e.preventDefault();
    const parsed = parseChat(chat);
    if (parsed === "help") model.toast("Commands: /p /invite /accept /leave /mute /unmute /report");
    else if (parsed) session.send(parsed);
    setChat("");
    chatRef.current?.blur();
    canvasRef.current?.focus();
  };

  const me = model.meView;
  const sheet = model.sheet;
  const now = performance.now();
  const stale = now - model.lastSnapshot > 1500;
  const rtt = session.rtt;
  const netColor = stale ? "blood_hi" : rtt === null ? "ash" : rtt < 120 ? "sickly_hi" : rtt < 250 ? "amber" : "blood_hi";
  const pos = model.renderPos(model.me, now);
  let remnant: string | null = null;
  if (me?.remnant && pos) {
    const dx = (me.remnant[0] - pos[0]) / CELL, dy = (me.remnant[1] - pos[1]) / CELL;
    const d = dirFromOffset(Math.round(dx), Math.round(dy));
    remnant = `◊ remnant ${Math.round(Math.hypot(dx, dy))} ${d ? d.toUpperCase() : "here"}`;
  }
  const equipName = (id: string | null) => (id ? sheet.inventory.find((i) => i.item === id)?.name ?? id : "—");

  return (
    <div className={`game${narrow ? " narrow" : ""}`}>
      <section className="hud" aria-label="status">
        {me && (
          <>
            <Bar label="HP" cur={me.hp} max={me.hp_max} fg="blood_hi" back="blood" />
            <Bar label="ST" cur={me.stamina} max={me.stamina_max} fg="sickly" back="moss" />
            <Bar label="WK" cur={me.wick} max={me.wick_max} fg="fog" back="water" />
            <span style={{ color: shade("amber", 1) }}>flask {me.draughts}/{me.draughts_max}</span>
            <strong style={{ color: uiColor("echoes") }}>✧ {me.echoes}</strong>
            <div className="info">
              <span style={{ color: shade("fog_hi", 1) }}>{model.zoneName}</span>
              {me.interact && <span style={{ color: shade("amber_hi", 1) }}>[F] {me.interact}</span>}
              {remnant && <span style={{ color: shade("sickly_hi", 1) }}>{remnant}</span>}
              {me.state === "guarding" && <span className="muted">(guarding)</span>}
              {me.state === "staggered" && <span className="muted">(staggered)</span>}
              {(me.cooldowns[0] > 0 || me.cooldowns[1] > 0) && (
                <span className="muted">rites {Math.ceil(me.cooldowns[0] / 30)}s/{Math.ceil(me.cooldowns[1] / 30)}s</span>
              )}
              <span className="net" style={{ color: shade(netColor, 1) }} aria-label="network">
                {stale ? "○ waiting for server" : rtt === null ? "○ …" : `● ${Math.round(rtt)} ms`}
              </span>
            </div>
          </>
        )}
      </section>

      <section className="viewport">
        <canvas ref={canvasRef} tabIndex={0} aria-label="The world of Veyr. Keyboard controls: WASD to move, J and K to attack, Space to roll, F to interact, question mark for help." />
        <div className="toasts" aria-live="polite">
          {model.toasts.slice(-4).map((t) => <div className="toast" key={t.at}>{t.text}</div>)}
        </div>
        {me?.boss && (
          <div className="bossbar" role="status">
            <span className="name">{me.boss.name}{me.boss.phase >= 2 ? " — cracked" : ""}</span>
            {me.boss.vulnerable && <span className="open">OPEN — STRIKE</span>}
            <div className="track"><div className="fill" style={{ width: `${(me.boss.hp / Math.max(1, me.boss.hp_max)) * 100}%` }} /></div>
          </div>
        )}
      </section>

      {!narrow && (
        <aside className="side" aria-label="character">
          <div><strong style={{ color: shade("amber_hi", 1) }}>{sheet.name}</strong> <span className="muted">level {sheet.level}</span></div>
          <div className="muted">next level {sheet.next_level_cost} ✧</div>
          <h3>Equipped</h3>
          <dl>
            <dt>hand</dt><dd>{equipName(sheet.equipment.main)}</dd>
            <dt>off</dt><dd>{equipName(sheet.equipment.off)}</dd>
            <dt>body</dt><dd>{equipName(sheet.equipment.armor)}</dd>
            <dt>rite 1</dt><dd>{equipName(sheet.equipment.spell1)}</dd>
            <dt>rite 2</dt><dd>{equipName(sheet.equipment.spell2)}</dd>
          </dl>
          <h3>Party</h3>
          {model.party.length === 0 && <div className="muted">alone. type /invite name</div>}
          {model.party.map((p) => (
            <div key={p.name} style={{ color: shade("sickly_hi", 1) }}>
              {p.leader ? "★ " : ""}{p.name}{p.hp ? ` ${Math.round((p.hp[0] / Math.max(1, p.hp[1])) * 100)}%` : ""}
            </div>
          ))}
          {model.invites.map((i) => (
            <div key={i} className="row" style={{ marginTop: 4 }}>
              <span style={{ color: shade("amber", 1) }}>invite from {i}</span>
              <button onClick={() => session.send({ t: "party_accept", from: i })}>Accept</button>
            </div>
          ))}
          <h3>Path</h3>
          {sheet.quests.filter((q) => !q.done).length === 0 && <div className="muted">Speak with those who remember.</div>}
          {sheet.quests.filter((q) => !q.done).map((q) => (
            <div key={q.id} style={{ marginBottom: 8 }}>
              <div className="gold">{q.title}</div>
              <div>{q.note}</div>
            </div>
          ))}
          <h3>Keys</h3>
          <div className="muted">WASD move · J/K attack · Space roll · L guard · I parry · H drink · 1/2 rite · F use · Tab gear · V level · M map · ? help</div>
        </aside>
      )}

      <section className="chat" aria-label="chat">
        <ul className="log" ref={logRef} aria-live="polite">
          {model.chat.slice(-80).map((c, i) => (
            <li key={`${c.at}-${i}`} className={c.channel === "party" ? "chan-party" : c.channel === "system" ? "chan-system" : ""}>
              {c.channel === "party" && "[party] "}
              {c.from && <span className="from">{c.from}: </span>}
              {c.text}
            </li>
          ))}
        </ul>
        <form onSubmit={submitChat}>
          <label htmlFor="chat-input" className="muted" style={{ alignSelf: "center" }}>say</label>
          <input id="chat-input" ref={chatRef} type="text" value={chat} maxLength={MAX_CHAT_CHARS}
            onChange={(e) => setChat(e.target.value)} placeholder="Enter to talk · /p party · /invite name"
            onFocus={() => inputRef.current?.release()} autoComplete="off" />
        </form>
      </section>

      {dialogueOpen && model.dialogue && (
        <div className="overlay" role="presentation">
          <div className="dialog speech" role="dialog" aria-modal="true" aria-labelledby="dlg-speaker">
            <h2 id="dlg-speaker">{model.dialogue.speaker}</h2>
            {model.dialogue.lines.map((l, i) => <p key={i}>{l}</p>)}
            <div className="choices">
              {model.dialogue.choices.map((c, i) => (
                <button key={i} autoFocus={i === 0} onClick={() => session.send({ t: "talk", npc: model.dialogue!.npc, choice: i })}>{i + 1}. {c}</button>
              ))}
              <button autoFocus={model.dialogue.choices.length === 0} onClick={() => { model.dialogue = null; setTick((t) => t + 1); canvasRef.current?.focus(); }}>
                {model.dialogue.choices.length ? "Leave" : "Close"}
              </button>
            </div>
          </div>
        </div>
      )}

      {overlay !== "none" && (
        <div className="overlay" role="presentation" onClick={(e) => { if (e.target === e.currentTarget) { setOverlay("none"); canvasRef.current?.focus(); } }}>
          {overlay === "character" && (
            <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="gear-title">
              <h2 id="gear-title">Gear and echoes</h2>
              <div className="grid2">
                <ul className="items" aria-label="inventory">
                  {sheet.inventory.map((it, i) => {
                    const equipped = Object.values(sheet.equipment).includes(it.item);
                    return (
                      <li key={it.item}>
                        <button aria-current={i === sel} onClick={() => setSel(i)}>
                          {equipped ? "E " : "  "}{it.name}{it.qty > 1 ? ` x${it.qty}` : ""}
                        </button>
                      </li>
                    );
                  })}
                </ul>
                <div>
                  {sheet.inventory[sel] && (() => {
                    const it = sheet.inventory[sel];
                    const eq = sheet.equipment;
                    const slot: EquipSlot | null = it.kind === "weapon" ? "main" : it.kind === "shield" ? "off" : it.kind === "armor" ? "armor" : it.kind === "rite" ? "spell1" : null;
                    const equip = (s: EquipSlot) => session.send({ t: "equip", slot: s, item: eq[s] === it.item ? null : it.item });
                    return (
                      <>
                        <p className="gold"><strong>{it.name}</strong> <span className="muted">({it.kind})</span></p>
                        <p>{it.desc}</p>
                        <div className="row">
                          {slot && <button onClick={() => equip(slot)}>{eq[slot] === it.item ? "Unequip" : "Equip"}</button>}
                          {it.kind === "rite" && <button onClick={() => equip("spell2")}>{eq.spell2 === it.item ? "Unequip slot 2" : "Prepare in slot 2"}</button>}
                          {it.kind === "consumable" && <button onClick={() => session.send({ t: "use_item", item: it.item })}>Use</button>}
                        </div>
                      </>
                    );
                  })()}
                  <p style={{ color: uiColor("echoes") }}>level {sheet.level} · echoes {sheet.echoes} · next {sheet.next_level_cost}</p>
                  <dl className="keys">
                    {ATTRIBUTES.map((a, i) => (<Fragment key={a}><dt className="muted">{a}</dt><dd style={{ margin: 0 }}>{sheet.attrs[i]}</dd></Fragment>))}
                  </dl>
                  <p className="muted">health {sheet.stats.hp_max} · stamina {sheet.stats.stamina_max} · wick {sheet.stats.wick_max} · defense {sheet.stats.defense} · light {sheet.stats.light_damage} · heavy {sheet.stats.heavy_damage} · rite {sheet.stats.spell_power}</p>
                  {sheet.quests.map((q) => (
                    <p key={q.id}><span className={q.done ? "muted" : "gold"}>{q.done ? "✓" : "·"} {q.title}</span>{!q.done && <><br />{q.note}</>}</p>
                  ))}
                </div>
              </div>
              <div className="row"><span className="spacer" /><button onClick={() => { setOverlay("none"); canvasRef.current?.focus(); }}>Close</button></div>
            </div>
          )}
          {overlay === "level" && (
            <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="lvl-title" style={{ width: "min(520px,100%)" }}>
              <h2 id="lvl-title">Invest echoes</h2>
              <p style={{ color: uiColor("echoes") }}>Echoes {sheet.echoes} · next level costs {sheet.next_level_cost}</p>
              <div className="choices">
                {ATTRIBUTES.map((a, i) => (
                  <button key={a} disabled={sheet.echoes < sheet.next_level_cost} onClick={() => session.send({ t: "level_up", attr: a })}>
                    {a} {sheet.attrs[i]} → {sheet.attrs[i] + 1}
                  </button>
                ))}
              </div>
              <p className="muted">Only at a lit lantern you stand beside.</p>
              <div className="row"><span className="spacer" /><button onClick={() => { setOverlay("none"); canvasRef.current?.focus(); }}>Close</button></div>
            </div>
          )}
          {overlay === "map" && (
            <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="map-title" style={{ width: "min(1100px,100%)" }}>
              <h2 id="map-title">Map of Veyr</h2>
              <MapView model={model} />
              <div className="row"><span className="spacer" /><button onClick={() => { setOverlay("none"); canvasRef.current?.focus(); }}>Close</button></div>
            </div>
          )}
          {overlay === "help" && (
            <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="help-title">
              <h2 id="help-title">How to stay alive</h2>
              <dl className="keys">
                <dt><kbd>WASD / arrows</kbd></dt><dd>move (Q E Z C diagonals, or hold two keys)</dd>
                <dt><kbd>J / K</kbd></dt><dd>light / heavy attack — turns toward a foe in reach</dd>
                <dt><kbd>Space</kbd></dt><dd>dodge roll: a few frames of invulnerability, costs stamina</dd>
                <dt><kbd>L</kbd></dt><dd>hold to guard</dd>
                <dt><kbd>I</kbd></dt><dd>parry with a shield: tight window, staggers the attacker</dd>
                <dt><kbd>H</kbd></dt><dd>drink a tallow draught (slow, can be interrupted)</dd>
                <dt><kbd>1 / 2</kbd></dt><dd>cast prepared rites</dd>
                <dt><kbd>F</kbd></dt><dd>talk, rest at a lantern, open doors, touch walls</dd>
                <dt><kbd>Tab · V · M</kbd></dt><dd>gear · invest echoes · map</dd>
                <dt><kbd>Enter</kbd></dt><dd>chat: /p party /invite /accept /leave /mute /report</dd>
                <dt><kbd>Esc</kbd></dt><dd>menu and settings</dd>
              </dl>
              <p className="muted">A <strong>!</strong> over a foe means it is winding up. Tinted cells show where it lands; brighter, bolder cells hit sooner.</p>
              <p className="legend"><span>× strike</span><span>▓ heavy</span><span>░ area</span><span>◌ shockwave — roll through</span><span>≈ lingering</span><span>+ weak point</span></p>
              <p className="muted">Dying leaves your echoes as a ◊ where you fell. Die again first and they are gone.</p>
              <div className="row"><span className="spacer" /><button autoFocus onClick={() => { setOverlay("none"); canvasRef.current?.focus(); }}>Close</button></div>
            </div>
          )}
          {overlay === "menu" && (
            <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="menu-title" style={{ width: "min(460px,100%)" }}>
              <h2 id="menu-title">Menu</h2>
              <div className="choices">
                <button autoFocus onClick={() => { setOverlay("none"); canvasRef.current?.focus(); }}>Resume</button>
                <button onClick={() => setSettings({ ...settings, ascii: !settings.ascii })}>Glyphs: {settings.ascii ? "ASCII" : "Unicode"}</button>
                <button onClick={() => setSettings({ ...settings, highContrast: !settings.highContrast })}>High contrast: {settings.highContrast ? "on" : "off"}</button>
                <button onClick={() => setSettings({ ...settings, reducedMotion: !settings.reducedMotion })}>Reduced motion: {settings.reducedMotion ? "on" : "off"}</button>
                <div className="row">
                  <span>Text size {settings.fontSize}px</span>
                  <button aria-label="smaller" onClick={() => setSettings({ ...settings, fontSize: Math.max(11, settings.fontSize - 1) })}>−</button>
                  <button aria-label="larger" onClick={() => setSettings({ ...settings, fontSize: Math.min(26, settings.fontSize + 1) })}>+</button>
                </div>
                <button onClick={() => { setOverlay("none"); session.send({ t: "leave" }); }}>Leave to character select</button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
