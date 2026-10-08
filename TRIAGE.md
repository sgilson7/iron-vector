# Triage

Stage 7, written by the person after using the build. `C4 Verify against hand-computed cases`.

Suggested first pass, with values you can check by hand from `data/parts.json`:

1. Garage: hover LG-08 WISP in the legs slot. Weight should read 79100 → 72100 (59 100 carried + 13 000).
2. Test field: hold W. Speed should settle near 95 km/h (the default legs' `walk_kmh`).
3. Test field: press C, then hold W. Speed should approach 230 km/h (BT-3 SURGE's `glide_kmh`).
4. Fit RJ-05 HARE legs. The garage should warn OVERWEIGHT (59 100 carried > 52 000 load limit), and walking should drop to about 74 km/h.
5. Play Operation Saltline to the end and note the time, AP kept and rank.

One row per concrete change. Say what you did, what you expected, and what happened. Compare at least one result with a case you worked out by hand.

| # | What I did | Expected | What happened | Change wanted | Status |
| --- | --- | --- | --- | --- | --- |
| 1 | | | | | |
