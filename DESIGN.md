# joy — design brief

> **Too many plans. One jar.**

## The concept: the date jar

Couples really do this. They write date ideas on little slips of paper, fold them, drop them in a jar and pull one out when they can't decide. Joy *is* that jar, made digital without losing the paper, the ink or the small ceremony of drawing a slip.

The metaphor maps one-to-one onto what the app does. **Do not invent mechanics the app doesn't have.**

| App mechanic | In the jar world | Copy |
|---|---|---|
| Wishlist (`/wishlist`) | **The slips**: every idea is a slip of paper | "Slips", "Write a slip" |
| Filters (`/`) | Telling the jar what tonight looks like | "Tonight we have…" |
| Pick (`POST /pick`) | **Drawing a slip** out of the jar | "Draw a slip" |
| Reroll (max 3) | **Putting it back**, using one of three *peeks* | "Put it back" |
| Accept (`POST /accept/{id}`) | The slip gets **stamped**: IT'S A DATE | "It's a date" |
| History (`/history`) | **The keepsake box**: stamped slips you kept | "Kept" |
| Archive | Taking a slip out of the jar | "Take out" |

Name for the nav: **Jar · Slips · Kept**.

## What we want them to feel, moment by moment

| Moment | Feeling | What happens |
|---|---|---|
| Login | *Intimacy, anticipation.* A private shelf with one jar on it. | The jar's lid is on. The right passcode twists it open and you're in. A wrong one makes the lid wobble "nope" with no shouting. |
| Filters | *Playful conversation between two people.* | Filters read as a sentence you fill in together: *"Tonight we have ₹2,000 and about half a day, around town, no further than 30 km from Neha's place, feeling cozy."* Blanks are tappable. As you change them, slips that don't fit **dim and settle** in the jar, and a quiet counter says *"14 slips could happen tonight."* |
| Draw | *Suspense, then delight.* **Big moment #1.** | The jar gives a little shake, one slip rises out of the mouth and unfolds into the card. A tap anywhere skips straight to the card. |
| Put it back | *Lightness, no guilt.* | The slip folds and drops back in. One of three peek tokens empties. After the last: *"No more peeking. This one's yours."* |
| Accept | *Commitment, celebration.* **Big moment #2.** | A vermilion rubber stamp thumps onto the slip: **IT'S A DATE** with today's date. The paper jolts a little, the phone buzzes once (`navigator.vibrate(12)`) and a couple of tiny ink hearts fly off. Then calm. |
| Nothing fits | *Gentle help, never a dead end.* | A handwritten note: the hint text plus **one button that applies the fix**. The server provides `relax_label` and `relax_hidden`. |
| Kept | *Nostalgia, warmth.* | A scrapbook. Stamped slips are taped onto pages by month, slightly rotated, with ratings as hand-drawn hearts and notes in italic serif. *"7 dates kept."* |
| Slips (wishlist) | *Calm, tidy, yours.* | Slips laid out in a loose stack grouped by where they happen. Writing a new slip feels like writing on paper. Saving folds it. |
| Empty jar (0 slips) | *Invitation.* | "The jar is empty. Write the first slip." |

## Art direction

**Point of view: tactile and intimate.** Paper, ink, a rubber stamp, a glass jar. Mostly quiet pages, so the one vermilion stamp and the one glowing jar feel significant. Restraint first: if everything moves, nothing matters.

Texture comes from real things, used deliberately: paper grain (an SVG `feTurbulence` noise filter at very low opacity on slips only, not the page), a slight torn or deckled slip edge (`clip-path` or mask), the stamp's uneven ink (an SVG mask). No gradients for their own sake, no glassmorphism, no rounded SaaS feature cards, no emoji as icons in the new design. Draw the few icons you need as inline SVG with a hand-drawn ink stroke.

**Composition.** Not everything is centred and equal. The jar can be big and off-axis, and slips can sit at 1–3° rotations. Body copy stays straight and readable.

**Night mode = the jar at night.** In dark mode the page is deep ink-indigo and the jar glows faintly like a lantern full of fireflies. Slips catch a little warm light. This is not inverted light mode. It's the same world after sunset.

## Tokens (in `static/tokens.css`; use the variables and never hard-code colours)

| Token | Light | Dark | Use |
|---|---|---|---|
| `--paper` | `#f5eee3` | `#14121b` | page |
| `--paper-2` | `#ebe2d3` | `#1d1a26` | wells, the jar shelf |
| `--slip` | `#fffaf2` | `#2a2533` | slips, cards |
| `--ink` | `#2b2433` | `#f1e9df` | text (≥ 12:1 on paper) |
| `--ink-soft` | `#675d6e` | `#b3a8b8` | secondary text (≥ 4.5:1 on paper and slip) |
| `--rule` | `#ddd1c1` | `#383145` | hairlines |
| `--stamp` | `#b9301b` | `#ff7a63` | **the one accent**: stamp, primary action, focus |
| `--stamp-ink` | `#fffaf2` | `#1a0f0c` | text on `--stamp` |
| `--glow` | `rgba(255,190,110,.0)` | `rgba(255,190,110,.35)` | jar lantern glow (dark only) |
| `--tint-home` | `#f3e2b3` | `#4a3f22` | slip tint by setting |
| `--tint-local` | `#f2cfc6` | `#4d2f2b` | |
| `--tint-day_trip` | `#d3e1c6` | `#2f4030` | |
| `--tint-travel` | `#cbdbea` | `#283a4d` | |

Tints are for the slip's folded edge and small marks only, never behind body text unless contrast stays at least 4.5:1.

## Type (self-hosted in `static/fonts/`; never load fonts from a CDN)

- **Fraunces** (`--font-display`): headlines, slip titles on the drawn card and big numbers. Variable axes: `opsz` 9–144, `wght` 300–900, `SOFT` 0–100, `WONK` 0–1. Use `SOFT 100` and `WONK 1` for warmth at display sizes. Italic for notes and quotes.
- **Instrument Sans** (`--font-ui`): all UI, labels, body and buttons.
- **Caveat** (`--font-hand`): *accents only*. Use it for handwritten annotations, the "peeks" label, little marginalia and the hearts caption. **Never use it for notes, body text or anything that must be read at under 20px.**

Scale: `--step--1` .875rem · `--step-0` 1rem · `--step-1` 1.25rem · `--step-2` 1.6rem · `--step-3` 2.2rem · `--step-4` 3.2rem · `--step-5` 4.6rem.

## Motion vocabulary

| Token | Value | For |
|---|---|---|
| `--t-micro` | 120ms | press states, toggles |
| `--t-small` | 220ms | hovers, chips, small reveals |
| `--t-medium` | 420ms | slip fold and unfold, page elements entering |
| `--t-hero` | 1100ms | **only** the draw, the stamp and the login lid |
| `--ease-out` | `cubic-bezier(.2,.8,.2,1)` | default |
| `--ease-spring` | `cubic-bezier(.34,1.56,.64,1)` | overshoot: stamp landing, slip settling |
| `--ease-in-out` | `cubic-bezier(.65,0,.35,1)` | shakes, travel |

Rules:
1. Only three hero animations exist: **draw**, **stamp** and **lid**. Everything else is small or medium.
2. **A tap or keypress skips any hero animation** to its end state. Nothing ever blocks reading or acting for more than about 1.2s.
3. `prefers-reduced-motion: reduce`: no shake, no physics and no flying. Use crossfades of 150ms or less. All content and results still appear.
4. Any `requestAnimationFrame` or canvas loop **stops when the tab is hidden** (`visibilitychange`) and when nothing is moving.
5. No scroll-jacking, no parallax and no autoplay sound.

## Hard invariants (do not break these)

- **Reroll stays stateless.** The pick card's form posts the `hidden` fields back to `/pick` with `reroll=1`. The server enforces the cap.
- **Lock while busy.** Use `hx-disabled-elt="this"` and/or `hx-sync="closest form:drop"` on draw, put back and accept, and keep it locked while the animation plays. A double-tap must never record twice.
- The result region keeps `aria-live="polite"`.
- Contrast is at least 4.5:1 for text in both themes. Tap targets are at least 44×44px. Focus is visible as a `--stamp` 2px outline with offset.
- Real `<label>`s for every input. The "sentence" filters must still be real form controls, such as `<select>` or radio groups styled inline.
- Must work with **0, 3, 40 and 200** slips, and with long titles like *"Hindi rap listening session (watch her react)"*.
- No runtime requests to any CDN. Everything lives in `static/`. htmx is at `/static/htmx.min.js`.
- Mobile-first. 375px wide is the primary canvas with 16px side gutters and no horizontal scroll. Desktop is a nice-to-have that should still look composed.

## Server contract (what templates receive)

The templates are Askama (Jinja-like). Missing or renamed fields are compile errors.

- `decide.html` (`DecideTpl`): `couple` ("Rohit & Neha"), `currency`, `count`, `slips_json` (a **pre-escaped** JSON array of `{id,title,setting}`; render it with `{{ slips_json|safe }}` inside `<script type="application/json" id="slips">`), and `durations`, `settings`, `tags`, `energies`, `kms`, `froms`: each is a `Vec<Opt{value,label,selected}>`.
  - Form field names: `budget`, `max_duration`, `settings` (multi), `tag`, `energy`, `max_km`, `from`.
  - `POST /match` with the same form body returns JSON `{count,total,ids}`, the slips that fit. Use it for the live counter and for dimming.
  - `POST /pick` returns the `pick_card.html` partial.
- `pick_card.html` (`CardTpl`): `activity: Option<ActivityView>`, `message`, `relax_label`, `relax_hidden: Vec<(String,String)>`, `done: bool`, `stamp_date`, `rerolls_left`, `max_rerolls`, `hidden: Vec<(String,String)>`.
  - When `relax_label` is non-empty, render a form posting `relax_hidden` to `/pick`.
  - `ActivityView` fields: `id, title, notes, place, cost, duration, setting, setting_key, tags: Vec<String>, distances: Vec<String>, maps_url: Option<String>, times_done`.
- `wishlist.html` (`WishlistTpl`): `groups: Vec<Group{key,label,items: Vec<ActivityView>}>`, `archived: Vec<ActivityView>`, `total`. Actions: `POST /wishlist/{id}/archive`, `/restore`, `/delete`; edit is at `GET /wishlist/{id}`; new is at `GET /wishlist/new`.
- `activity_form.html` (`FormTpl`): `id: Option<i64>`, `title, notes, est_cost, venue, area, location, tags, currency, error`, plus `settings, durations, energies, people: Vec<Opt>`. Posts to `/wishlist` or `/wishlist/{id}`, with fields `title, est_cost, duration, setting, energy, venue, area, location, tags, notes, added_by`.
- `history.html` (`HistoryTpl`): `months: Vec<Month{label, items: Vec<HistoryRow>}>`, `kept`, `skipped: Vec<HistoryRow>`.
  - `HistoryRow` fields: `id, title, setting, place, day, outcome, rating: Option<i64>, note`.
  - `HistoryRow` methods: `day_label()` ("14 Oct"), `month_label()`, `is_rated(n)`.
  - Rating: `POST /history/{id}` with `rating` (1–5 or "") and `note`.
- `login.html` (`LoginTpl`): `error`. `POST /login` with `passcode`.
- `base.html`: blocks `title`, `head` (extra CSS and JS per page), `nav`, `content`. It loads `tokens.css`, which carries the fonts and tokens.
