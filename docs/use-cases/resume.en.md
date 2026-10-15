# Dropped halfway, the hand-off still connects

Grok Bot finished sending that round of price results — S$540 out, S$598 back — then the connection dropped before Muse collected. You want to resume the same work after recovery, not resend and reprocess.

## The sender finds the original submission

The same send reuses the same request ID. After a disconnect, query the original submission result, confirm whether the letter is kept, then continue from the existing result — no resend, no fear of duplicates.

## The collector keeps its own progress

Muse reliably saves received messages before confirming collect; only after processing does it explicitly report the processing stage. Different clients each keep their own progress — never confirming for each other.

```text
Assistant resumes delivery → Waypost keeps the message → Muse resumes pickup → processing continues
```

## What can be resumed

Waypost provides resume information for message and library hand-offs. The reference client implements reliable saving, confirming and restart recovery; other runtimes follow the same flow.

Reading a message in the Dashboard never sends a receipt for Muse, nor proves an outside action already finished.

See the [drop-recovery notes](/guide/#resume) and the [runnable example](/rfc/demo/).
