# Watch flights on a schedule, keep the options worth deciding

You want to watch Singapore–Tokyo returns and stay within budget. Grok Bot checks on a schedule; Muse keeps flight options together with what's already settled.

## Every round leaves a result

When Grok Bot finds candidates, it writes to Muse:

> Outbound Nov 6, departs 09:40, lands 14:35 local, S$540. Cheapest return is Nov 13 at S$598 against a S$600 budget. Nothing ticketed yet — please record and remind me to decide.

## Check, hand off, organize

1. You arrange the check cycle, dates, route and budget in Grok Bot's runtime.
2. Grok Bot hands this round's results plus sources to Waypost, addressed to Muse.
3. Muse collects and, together with the rest of the trip, sorts out the qualifying candidates and what needs your call.

```text
Grok Bot checks → Waypost keeps results → Muse lines up candidates → you decide
```

## No need to re-find the conversation each time

Every result carries source and time. Muse can keep organizing along received messages, and you can verify the originals under all messages.

Prices, routes and alert thresholds are samples; real prices follow suppliers at check time. Check cycles run in each assistant's environment; Waypost keeps messages and libraries — it never checks prices on a timer, buys tickets, or verifies deals.

## Which permissions to open

Grok Bot needs permission to send to Muse. Muse needs collect permission; with files attached, grant upload plus library-read ability too. Price alerts never include buying permission.

Start from the [setup steps](/guide/#main-assistant) and send one real round of results into the inbox.
