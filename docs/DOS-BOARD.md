# How Dos's scheduled tasks talk to Dos Live

Dos Live reads the **Dos** Trello board every minute. The scheduled tasks
(Morning Briefing, Trello Check-In, Weekly Priority Report, Working Dashboard
Refresh, and later the watchdog) need no changes to be picked up as pings. To
ask Elijah a question, add the block below to a task's prompt.

## Pings (already working)

Any card on the Dos board whose title starts with `🔔`, `Morning Briefing`,
`Trello Check-In`, `Weekly Priority Report`, `Working Dashboard Refresh` or
`Watchdog` is a ping. When a new one appears (created in the last 24 hours),
Dos Live:

- drops the panel open with the ping's first sentence,
- plays the ping cue and reads the first sentence out loud (unless he's on a call),
- links **Open** to the first `https://claude.ai/…` link in the description
  (the rendered briefing page), or to the card.

Keep the first paragraph of the description a plain one-line summary. That
line is what gets read out loud.

## Asking for a go-ahead

Paste this into any scheduled task that should ask before acting:

> **Asking Elijah through Dos Live.** When an action needs his approval (anything
> outside Dos's allowed-without-asking list), don't do it and don't only mention
> it in the ping. Create one card per decision in the **Dos asks** list on the Dos
> board: the title is the question, phrased so "do it" is an answer (e.g. "Send
> the drafted reply to the landlord?"); the description's first paragraph is the
> context in one or two sentences. Label it with the world's label if one fits.
>
> **Reading his answers.** At the start of every run, read the **Dos answered**
> list. Each card title now starts with his answer:
> - `✅ DO IT — …` → do it now, then archive the card and mention it in your ping.
> - `⏸ HOLD — …` → leave it; ask again only if something changes.
> - `✖ SKIP — …` → drop it and archive the card.
>
> Never act on a card still sitting in **Dos asks**.

Dos Live creates both lists for you: **Settings → Trello → Create the "Dos asks" lists**.

## Notes

"Dos, note buy cement for Matuu" (or typing `note …` at the panel's prompt)
adds a `📝 buy cement for Matuu` card at the top of the Dos board's **To do**
list. Tasks can treat 📝 cards as inbox items to route to the right board.
