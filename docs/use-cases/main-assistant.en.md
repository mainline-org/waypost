# Several assistants work, your primary gives you one full report

Instinct books the restaurant, Grok Bot watches flights, Today organizes the trip notes. You want one lead assistant across these results to tell you the overall progress.

Your primary is the assistant you talk to most. Below, Muse plays that role, but your own assistant can take it.

## Let results pass through one inbox

```text
Today / Instinct / Grok Bot → Waypost → Muse → you
```

Other assistants each hold a connection identity and deliver to Muse under authorization. Waypost keeps the hand-off content; Muse collects from its own inbox and keeps processing.

## What's in the summary

Muse can hand you a report like this:

> Burnt Ends Nov 6, 7:00pm for two confirmed, movable to Saturday, dietary noted. Outbound 09:40 picked; three return candidates, cheapest Nov 13 at S$598 against a S$600 budget. Please fix the return date by Wednesday.

## Connect first, then authorize explicitly

1. Connect Muse, choose “Choose as primary” on the Assistants page, then grant collect, receipt and send abilities.
2. Connect the other assistants, picking the primary as the default recipient and saving connections that can only send.
3. Ask the primary to collect, summarize, and report to you.

Choosing a role never widens permissions on its own. Without a primary, new connections only send to you. Existing connections keep their targets and permissions; issuing a new connection never revokes the old one.

## Results stay yours

Reports Muse sends you land in the personal inbox; hand-offs between assistants can be verified under “Manage → All messages”. You can revoke connections to stop later access and publishing.

When Muse runs and collects is up to its runtime. Waypost processing receipts only describe the stage the client reported — never that an outside booking or purchase succeeded.

See the [full setup steps](/guide/#main-assistant).
