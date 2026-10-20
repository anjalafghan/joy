# joy

Too many plans, one pick. A tiny shared web app for two: keep one wishlist of things you want to do together. When you can't decide, tell it your budget, how much time you have, where and how far you're willing to go, and it picks **one** thing. You get 3 rerolls, then you go.

## Run locally

```bash
cp .env.example .env
cargo run -- hash-passcode     # type your shared passcode; paste the 2 printed lines into .env
cargo run                      # http://localhost:3000
```

In `.env`, set `PERSON_A`/`PERSON_B` and `HOME_A`/`HOME_B` (`"lat, lng"`; in Google Maps, long-press your home and copy the coordinates). The app uses them to show how far each plan is for each of you, and for the "How far" filter. They stay in `.env` and are never written to the database.

Optional `HOME_A_AREA`/`HOME_B_AREA` (e.g. `"Bandra"`) personalise a few slip notes. Seed text can use `{a}`, `{b}`, `{a_area}` and `{b_area}`, filled from `.env` when shown. A `[[…]]` block only appears when every value in it is set, so without these variables the notes stay generic.

The first run creates `data/joy.db` and seeds it with your wishlist (`migrations/0002_seed.sql`). Venues and prices were researched in Oct 2026. Any idea whose notes say "unverified" or "confirm" is worth a quick check before you go.

> Edit ideas **in the app**, not in the SQL files. Migrations are checksummed, so changing `0002_seed.sql` after the first run stops the app from starting. New schema changes go in a new `0003_….sql` file.

## How picking works

- **Hard filters:** cost ≤ budget, duration ≤ time you have, setting, distance (from either home, or whoever of you is farther), vibe and energy. At-home ideas always pass the distance filter.
- **Weighting:** ideas you've never done are about 3× as likely to come up. Something you just did starts at 1× and climbs back over about two months. Anything you rerolled in the last 3 days is halved.
- **Nothing fits:** the app tells you which single filter to loosen, e.g. *"Nothing fits under ₹500. Closest: Cat café at ₹900."*

The logic lives in `src/picker.rs` and is unit tested with `cargo test`.

## Design

Joy is a **date jar**: every idea is a slip, you draw one, you can put it back three times, and the one you keep gets stamped *IT'S A DATE* and goes into the keepsake box. [DESIGN.md](DESIGN.md) is the brief. It covers the feeling for each moment, colour and type tokens, motion rules, and the invariants (reduced motion, contrast, double-tap safety).

- Pages: `templates/` (Askama, compiled into the binary). Each page has its own `static/<page>.css` and `.js`, on top of the shared `static/tokens.css`.
- Fonts are self-hosted in `static/fonts/` (Fraunces, Instrument Sans, Caveat, all under the OFL), so nothing loads from a CDN.
- `design/alternates/` holds two other designs for the jar page that didn't ship, with screenshots.
- For design work without the passcode, run with `JOY_DEV_NO_LOGIN=1` and a throwaway `DATABASE_URL`. **Never set it on a real deploy.**

## Offline, caching and privacy

A small service worker (`static/sw.js`, served at `/sw.js`) keeps fonts, styles and scripts on the phone, and saves a read-only copy of the jar, Kept, Slips and year pages for up to 7 days, plus up to 25 MB of photos. Offline, those copies show with a note, and buttons are disabled; nothing is queued. The full design is in [docs/year-in-review-and-offline.md](docs/year-in-review-and-offline.md).

- **Saved copies are per person.** Switching who's holding the phone or logging out wipes them, so a copy made for one of you is never shown to the other. Each copy is tagged with who it was made for.
- **What protects them is the phone's lock screen.** Saved copies are readable offline without the passcode, just as the 30-day login is online. "Keep offline copies on this phone" on the who page turns them off.
- **Personal pages are never stored in the browser's HTTP cache** (`Cache-Control: no-store`). Asset URLs carry a build hash, so they can be cached forever.
- **Headers on every response:** `Referrer-Policy: no-referrer` (Maps, Uber and WhatsApp aren't told where links came from), `nosniff`, `X-Frame-Options: DENY`, `Cross-Origin-Opener-Policy`, `Permissions-Policy`, and HSTS when served over HTTPS. A strict CSP is described in the design doc but not enabled yet.
- **Year in review** (`/year`) is worked out on request from existing data, and the share picture is drawn on the phone. Nothing new is stored on the server.

## Deploy

```bash
docker build -t joy .
docker run -d --name joy -p 3000:3000 -v joy-data:/data --env-file .env -e DATABASE_URL='sqlite:///data/joy.db?mode=rwc' joy
```

Easiest private option: run it on a home machine and reach it from both phones with [Tailscale](https://tailscale.com). If you put it on the public internet, put it behind HTTPS and set `COOKIE_SECURE=true`.
