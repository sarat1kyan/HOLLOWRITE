import { useEffect, useState, useSyncExternalStore } from "react";
import { currentUser, logout } from "./net";
import { Session } from "./game/session";
import AuthScreen from "./ui/AuthScreen";
import LobbyScreen from "./ui/LobbyScreen";
import GameScreen from "./ui/GameScreen";
import { loadSettings, saveSettings, type Settings } from "./settings";

type Stage = { kind: "boot" } | { kind: "auth"; error?: string } | { kind: "session"; session: Session };

function useSession(session: Session) {
  useSyncExternalStore(session.subscribe, session.snapshot);
  return session;
}

function Connected({ session, settings, setSettings, onSignOut }: {
  session: Session;
  settings: Settings;
  setSettings: (s: Settings) => void;
  onSignOut: () => void;
}) {
  const s = useSession(session);
  if (s.phase === "game" && s.model) {
    return <GameScreen session={s} settings={settings} setSettings={setSettings} />;
  }
  if (s.phase === "connecting") {
    return <main className="screen"><p className="muted" role="status">Connecting to the lantern…</p></main>;
  }
  return <LobbyScreen session={s} onSignOut={onSignOut} />;
}

export default function App() {
  const [stage, setStage] = useState<Stage>({ kind: "boot" });
  const [settings, setSettingsState] = useState<Settings>(loadSettings);
  const setSettings = (s: Settings) => {
    setSettingsState(s);
    saveSettings(s);
  };

  useEffect(() => {
    let cancelled = false;
    currentUser()
      .then((user) => {
        if (cancelled) return;
        setStage(user ? { kind: "session", session: new Session() } : { kind: "auth" });
      })
      .catch(() => !cancelled && setStage({ kind: "auth", error: "The server is unreachable." }));
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (stage.kind !== "session") return;
    const s = stage.session;
    return s.subscribe(() => {
      if (s.phase === "closed") {
        setStage({ kind: "auth", error: s.kicked ?? (s.closedReason ? `Disconnected: ${s.closedReason}` : undefined) });
      }
    });
  }, [stage]);

  if (stage.kind === "boot") return <main className="screen"><p className="muted" role="status">Listening for the bell…</p></main>;
  if (stage.kind === "auth") {
    return <AuthScreen initialError={stage.error} onSignedIn={() => setStage({ kind: "session", session: new Session() })} />;
  }
  const signOut = async () => {
    stage.session.close();
    await logout();
    setStage({ kind: "auth" });
  };
  return <Connected session={stage.session} settings={settings} setSettings={setSettings} onSignOut={signOut} />;
}
