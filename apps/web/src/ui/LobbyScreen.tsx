import { useState, type FormEvent } from "react";
import { APPEARANCE_HUES, APPEARANCE_SIGILS, ARCHETYPES, validateName, type Appearance, type Archetype } from "../protocol";
import type { Session } from "../game/session";
import { shade, theme } from "../game/theme";
import Title from "./Title";

export function PlayerGlyph({ look }: { look: Appearance }) {
  const g = theme.entities.player.glyphs[look.sigil % theme.entities.player.glyphs.length];
  return <span className="glyph" style={{ color: shade(theme.entities.hues[look.hue] ?? "bone", 1) }} aria-hidden="true">{g}</span>;
}

function CreateForm({ session, onCancel }: { session: Session; onCancel?: () => void }) {
  const [name, setName] = useState("");
  const [archetype, setArchetype] = useState<Archetype>("ash_warden");
  const [look, setLook] = useState<Appearance>({ sigil: 0, hue: 0 });
  const [error, setError] = useState<string | null>(null);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const err = validateName(name);
    if (err) return setError(err);
    setError(null);
    session.send({ t: "create_character", name, archetype, look });
  };

  return (
    <form className="panel" onSubmit={submit} aria-labelledby="create-title">
      <h2 id="create-title">A new Hollowbound</h2>
      <div className="field">
        <label htmlFor="cname">Name</label>
        <input id="cname" type="text" value={name} maxLength={16} onChange={(e) => setName(e.target.value)} autoFocus />
      </div>
      <fieldset className="archetypes" style={{ border: 0, padding: 0 }}>
        <legend className="muted">Origin</legend>
        {ARCHETYPES.map((a) => (
          <label key={a.id}>
            <input type="radio" name="archetype" value={a.id} checked={archetype === a.id} onChange={() => setArchetype(a.id)} />
            <strong className="gold">{a.label}</strong> — {a.blurb}
            <div className="muted">{a.stats}</div>
          </label>
        ))}
      </fieldset>
      <div className="field">
        <span className="muted" id="sigil-label">Sigil</span>
        <div className="swatches" role="group" aria-labelledby="sigil-label">
          {Array.from({ length: APPEARANCE_SIGILS }, (_, i) => (
            <button type="button" key={i} aria-pressed={look.sigil === i} aria-label={`sigil ${i + 1}`} onClick={() => setLook({ ...look, sigil: i })}>
              {theme.entities.player.glyphs[i]}
            </button>
          ))}
        </div>
      </div>
      <div className="field">
        <span className="muted" id="hue-label">Colour</span>
        <div className="swatches" role="group" aria-labelledby="hue-label">
          {Array.from({ length: APPEARANCE_HUES }, (_, i) => (
            <button type="button" key={i} aria-pressed={look.hue === i} aria-label={`colour ${i + 1}`}
              style={{ color: shade(theme.entities.hues[i], 1) }} onClick={() => setLook({ ...look, hue: i })}>■</button>
          ))}
        </div>
      </div>
      <p>You will appear as <PlayerGlyph look={look} /></p>
      <p className="danger" role="alert">{error ?? session.lobbyError ?? ""}</p>
      <div className="row">
        <button className="primary" type="submit">Wake</button>
        {onCancel && <button type="button" onClick={onCancel}>Back</button>}
      </div>
    </form>
  );
}

export default function LobbyScreen({ session, onSignOut }: { session: Session; onSignOut: () => void }) {
  const [creating, setCreating] = useState(false);
  const chars = session.characters;
  return (
    <main className="screen">
      <Title />
      {chars === null ? (
        <p className="muted" role="status">Listening for names…</p>
      ) : chars.length === 0 || creating ? (
        <CreateForm session={session} onCancel={chars.length > 0 ? () => setCreating(false) : undefined} />
      ) : (
        <section className="panel" aria-labelledby="chars-title">
          <h2 id="chars-title">{session.account} — characters</h2>
          <ul className="chars">
            {chars.map((c) => (
              <li key={c.id}>
                <button onClick={() => session.send({ t: "select_character", id: c.id })}>
                  <PlayerGlyph look={c.look} />
                  <strong>{c.name}</strong>
                  <span className="muted">{ARCHETYPES.find((a) => a.id === c.archetype)?.label} · level {c.level}</span>
                  <span className="spacer" />
                  <span className="muted">{c.location.replace(/_/g, " ")}</span>
                </button>
              </li>
            ))}
          </ul>
          {session.lobbyError && <p className="danger" role="alert">{session.lobbyError}</p>}
          <div className="row">
            <button onClick={() => setCreating(true)} disabled={chars.length >= 4}>New character</button>
            <span className="spacer" />
            <button onClick={onSignOut}>Sign out</button>
          </div>
        </section>
      )}
    </main>
  );
}
