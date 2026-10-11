const TITLE = ["╻ ╻┏━┓╻  ╻  ┏━┓╻ ╻┏━┓╻╺┳╸┏━╸", "┣━┫┃ ┃┃  ┃  ┃ ┃┃╻┃┣┳┛┃ ┃ ┣╸ ", "╹ ╹┗━┛┗━╸┗━╸┗━┛┗┻┛╹┗╸╹ ╹ ┗━╸"].join("\n");

export default function Title() {
  return (
    <header>
      <h1 className="title" aria-label="HOLLOWRITE">{TITLE}</h1>
      <p className="tagline">Death keeps what you forget.</p>
    </header>
  );
}
