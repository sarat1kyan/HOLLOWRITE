import { useState, type FormEvent } from "react";
import { authenticate } from "../net";
import { validateName } from "../protocol";
import Title from "./Title";

export default function AuthScreen({ initialError, onSignedIn }: { initialError?: string; onSignedIn: () => void }) {
  const [register, setRegister] = useState(false);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(initialError ?? null);
  const [busy, setBusy] = useState(false);
  const coarse = typeof matchMedia === "function" && matchMedia("(pointer: coarse)").matches && !matchMedia("(pointer: fine)").matches;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const nameError = validateName(username);
    if (nameError) return setError(nameError);
    if (register && password.length < 10) return setError("password must be at least 10 characters");
    setBusy(true);
    setError(null);
    try {
      const err = await authenticate(username, password, register);
      if (err) setError(err);
      else onSignedIn();
    } catch {
      setError("The server is unreachable.");
    } finally {
      setBusy(false);
      setPassword("");
    }
  };

  return (
    <main className="screen">
      <Title />
      {coarse && (
        <p className="notice" role="note">
          HOLLOWRITE is played with a keyboard. Touch-only devices are not supported in this version.
        </p>
      )}
      <form className="panel" onSubmit={submit} aria-labelledby="auth-title">
        <h2 id="auth-title">{register ? "Create account" : "Sign in"}</h2>
        <div className="field">
          <label htmlFor="username">Name</label>
          <input id="username" type="text" autoComplete="username" value={username} maxLength={16}
            onChange={(e) => setUsername(e.target.value)} autoFocus required />
        </div>
        <div className="field">
          <label htmlFor="password">Password</label>
          <input id="password" type="password" autoComplete={register ? "new-password" : "current-password"}
            value={password} maxLength={128} onChange={(e) => setPassword(e.target.value)} required />
        </div>
        <p className="danger" role="alert" aria-live="assertive">{error ?? ""}</p>
        <div className="row">
          <button className="primary" type="submit" disabled={busy}>{busy ? "…" : register ? "Create account" : "Sign in"}</button>
          <span className="spacer" />
          <button type="button" onClick={() => { setRegister(!register); setError(null); }}>
            {register ? "I have an account" : "New account"}
          </button>
        </div>
      </form>
    </main>
  );
}
