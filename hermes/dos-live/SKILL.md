---
name: dos-live
description: Reach Elijah through Dos Live, the Dos panel at the top of his Windows screen — show a message, say it out loud, or ask him a yes/hold/no question and wait for the answer.
---

# Dos Live

Dos Live runs on this Windows machine. `dosctl.exe` sits next to it
(default: `%LOCALAPPDATA%\Dos Live\dosctl.exe`) and talks to it over a local,
same-user-only pipe. Use it instead of Telegram whenever Elijah is likely at
his desk.

## Tell him something

```
dosctl say "Goat prices at Machakos market are up about 8% this month." --world farm --title "Market watch"
```

Add `--speak` to have Dos say it out loud. Add `--alert` only when it should
drop the panel open (something he'd want to know within the hour).

## Ask for a go-ahead

```
dosctl ask "Draft reply to the landlord is ready in Gmail. Send it?" --detail "Confirms October rent paid on the 1st." --timeout 300
```

It blocks until he answers, then prints `do_it`, `hold` or `skip`
(exit codes 0, 10, 11; 2 if nobody answered in time). Act only on `do_it`.
Treat a timeout exactly like `hold`.

## See what needs him

```
dosctl status
```

Returns each world's state and headline as JSON. Work is reduced to counts.

## Worlds

`finance`, `land`, `farm`, `creative`, `donstech`, `boom`. Omit `--world`
when nothing fits.

## Rules

- Never pass anything about Sportserve, Jira, tickets or his job. `--world work`
  is refused by Dos Live, and work details must not be in the text either.
- Keep `say` to one or two sentences; he reads it on a narrow bar.
- One `ask` at a time. Wait for the answer before asking another.
