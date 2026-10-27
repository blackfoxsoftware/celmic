---
version: 1
slug: "src-app-tsx"
primary_target: "src/App.tsx"
related_targets: []
---

## Scope

Mode: Operate. The desktop window (apps/desktop/src/App.tsx) and the phone app (apps/mobile/src/app), redesigned as one world with shared Tailwind v4 tokens (packages/ui-theme): shadcn/ui on desktop, React Native Reusables + Uniwind on the phone.

## Task

Desktop: while a phone streams, the "on air" status dominates; pairing, output and tuning stay one tab away. Phone: while streaming, a giant mute button dominates. Avoid: generic SaaS card dashboards, gamer RGB/neon, childish styling. Technical stats may stay visible.

## Direction contract

THESIS: celmic's state is a cassette J-card index stripe: one full-width colour band whose colour IS the state, with the phone's name set like a hand-labelled spine. It refuses the card grid with a green "live" dot.

OWN-WORLD: graphite shell ground (#16171a, rail #0f1012), J-card stock type (#eceae4). Three index inks, one state each and never ornament: vermilion = live / pressable, amber = paired but silent / needs attention, cobalt = waiting / pairing. Barlow Condensed labels and names, Barlow body, JetBrains Mono for every figure (tabular). Hairline-ruled track-list rows (A1, A2…) packed edge to edge; 6px radii; no card padding around data.

STORY: from across the desk the user reads whether the phone is on air and into which microphone; pairing is one guided step; stats wait for whoever looks.

FIRST VIEWPORT: desktop: 76px spine rail left (vertical wordmark, three-ink state key, host + IP), tabs Live / Pair / Output / Tuning with Disconnect right; a full-width index stripe ~236px tall in the state ink: "REC · ON AIR", phone name ~104px condensed, uptime on a tape counter right; below, a segmented voice-level meter and the track-list stats (left) beside an Output + pair column (Celmic Microphone, code counter, QR). Phone: the stripe as the top band, the mute button giant beneath.

FORM: Tape J-card, candidate 3 of 7 (ordered: ON AIR lightbox, Braun audio, Tape J-card, cue lights, FM station identity, backstage pass, test card). seed: none (unavailable). Signature move: tape-counter digits roll when uptime or the pairing code change, and the stripe re-inks when the state changes.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Raises kept from declined challengers

- Data field: every live figure in one tabular monospace.
- Warm consumer app: vermilion only where something is live or pressable.
- Orienteering map: each ink means exactly one state.
- Event catalog: stats packed edge to edge like a track list.

## Unresolved

- Localization (UI is English).
