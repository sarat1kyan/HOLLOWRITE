# ADR 0004: Movement input without reliable key-release events

- Status: accepted
- Date: 2026-10-11

## Context

Movement is held-state input. Browsers report key down and key up reliably.
Most terminals report only key presses plus OS auto-repeat; key release is
available only where the kitty keyboard protocol is supported (kitty,
WezTerm, foot, Ghostty, recent Alacritty and others).

## Decision

- The server receives movement as "current held direction" with a sequence
  number, plus a 12-tick time-to-live: if the client stops refreshing, the
  character stops on its own.
- The terminal client asks Crossterm whether keyboard enhancement is
  supported and, if so, enables release events and uses real held state.
- Otherwise it emulates holding: the first press holds the direction for
  480 ms (longer than common repeat delays), each repeat extends it by 140 ms.
  Diagonals are explicit keys (`Q E Z C`).
- Guard is a hold where release events exist and a toggle where they do not.
- The mode is selectable (`--input auto|hold|repeat` and the in-game menu).

## Consequences

- Movement feels right on modern terminals and acceptable elsewhere; a tap
  in repeat mode moves about 1.7 cells, which is documented in the help.
- The server never trusts the client to send a stop, so a dropped connection
  or a frozen terminal cannot leave a character running.
