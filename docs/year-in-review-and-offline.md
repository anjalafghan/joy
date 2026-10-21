# joy: "Our 2026" year in review + "Opens instantly, even offline"

Design document, not code. Written 2026-10-19 against commit `48b7402`. Browser facts were checked in October 2026 and cited at the end. Anything marked **(verify on device)** could not be confirmed from a primary source.

---

## 1. Summary and recommendation

**Keep the architecture.** Server-rendered Askama + htmx + SQLite on one Fly machine can support both features well. Add two small pieces:

1. **A year-in-review page** `GET /year/{yyyy}`, computed on request from the existing `history` and `activities` tables. It needs no new tables or files. A short script draws the share image **on the phone** with a `<canvas>`, and hands it to the share sheet (`navigator.share({files})`). Nothing is uploaded or stored on the server.
2. **A service worker** served from `/sw.js`, added as progressive enhancement:
   - **cache-first** for versioned static assets (about 665 KB, measured);
   - **network-first with a ~2 s timeout**, falling back to cache, for the three main pages `/`, `/history`, `/wishlist`;
   - an **offline fallback page**;
   - **Kept readable offline for up to 7 days**;
   - **read-only offline** (no queued writes in v1);
   - all page and photo caches **purged inside the worker** when the phone switches who's holding it (`POST /who`) or logs out (`POST /logout`).

**Why not local-first / CRDTs / client-side rendering:** the hard rule in this app is that a surprise is **sealed on the server** (`sealed_for` in `src/routes/decide.rs`). A client database on the partner's phone would either have to hold the sealed rows, which breaks the surprise as soon as anyone opens dev tools or a backup, or re-create per-viewer filtering inside the sync protocol. That is a lot of new risk for two users and a few hundred rows. The service worker approach keeps the server as the only place that decides what each person sees, and only caches what it already showed to *this* viewer.

**Be honest about "instantly".** Working hypothesis, which Phase 0 measures: most of today's wait on opening is **Fly's cold start**: the machine auto-stops when idle (`fly.toml`: `auto_stop_machines = 'stop'`, `min_machines_running = 0`). Fly's docs put a typical cold start at "~2+ seconds" and a resume from **suspend** at "a few hundred ms". A service worker hides this only for cached content. Recommended first step, before any service-worker code: **try `auto_stop_machines = 'suspend'`** (a one-line config change). It should be allowed on a 256 MB machine, but **(verify)** whether this machine qualifies.

**What it costs**

| | Number |
|---|---|
| New server storage | 0 bytes (no tables, no files) |
| Server CPU per `/year` request | One indexed query over ≤ a few hundred rows, well under 5 ms |
| Server load from the service worker | ~0. It *reduces* requests: no revalidation of static files, so fewer cold-start wakes |
| On-device precache | ≈ 665 KB measured (fonts 408 KB, CSS 118 KB, JS 61 KB, htmx 51 KB, icons 27 KB) |
| On-device page cache | 3 pages × ~20–60 KB each (estimate) |
| On-device photo cache | LRU cap **25 MB / 80 photos** (sized from an *estimated* 150–400 KB per 1600 px q82 JPEG) |
| First load | Unchanged. The worker registers after `load`, and its precache downloads in the background once |
| Share image in memory | 1080×1920 canvas ≈ 2.1 MP ≈ 8.3 MB RGBA. That is well under the iOS ~16.7 MP canvas cap |

**Decisions the user still needs to make** (recommended defaults in bold):
- Offline copies on by default? **Yes, capped at 7 days, with a per-phone "Don't keep offline copies" switch.**
- Offline writes? **No in v1** (read-only offline). Maybe later: queue rating + note only.
- Should logout also clear `joy_who`? **Yes** (see section 4.6).
- Place names on the share image? **Off by default**, one toggle.
- Security headers: section 5 lists recommendations only.

---

## 2. Architecture evaluation

| Option | Fits 11 (recap)? | Fits 12 (offline)? | Privacy (sealed surprises) | Cost to build/run | Verdict |
|---|---|---|---|---|---|
| **A. Today's SSR + htmx, plus a service worker (recommended)** | Yes: a normal page + a small canvas script | Good for reading. Writes need a network | Server stays the only gatekeeper. Cache only holds what this viewer was shown | Small: 1 route, 1 template, ~150 lines of SW JS, ~200 lines of canvas JS | **Do this** |
| B. Local-first: SQLite/IndexedDB on device + sync | Yes | Full offline, including writes | **Bad**: partner's device would sync sealed rows, or the sync needs a per-viewer filter (a new permission system) | Large: sync protocol, migrations on two clients, conflict rules | No |
| C. CRDTs (Automerge/Yjs) | Yes | Full offline, merging | Same problem as B, and CRDT history keeps deleted/sealed data unless compacted | Large, plus a JS bundle many times htmx's 51 KB | No |
| D. Client-side SPA (JSON API + framework) | Yes | Same as A once a SW is added | Same as A | Rewrite every template. No user-visible gain | No |

The current architecture already has the right properties for both features. Pages are small, the server is the single source of truth, and per-viewer views are computed on request. The only things missing are an on-device cache layer and asset versioning, and neither needs a redesign.

**Smallest change that meets the goals:**
1. Version static assets (build id in URLs) and serve them `immutable`.
2. Add `/sw.js` + `/offline` routes.
3. Add `/year/{yyyy}` + `year.html` + `static/year.js`.
4. Add a handful of response headers.

No database migration is needed.

---

## 3. Feature 11: "Our 2026" (year in review)

### 3.1 Shape

- **Route:** `GET /year/{yyyy}`, behind `require_login`. `/year` redirects to the current year (or to last year during January). Link to it from the top of Kept ("Our 2026 →") when the year has at least 3 kept dates.
- **Rendering:** a normal Askama page, `templates/year.html` + `static/year.css`. It is readable without JS. `static/year.js` adds only the share image.
- **Storage:** none. Everything is computed on request. The share image exists only in the phone's memory and the share sheet.
- **Same for both viewers:** the recap **leaves out every unrevealed surprise, for both people**. That covers counts, costs, months, streaks, tags and photos. If it didn't, the totals would leak a surprise ("why are there 14 dates when I count 13?"), and the *planner's* share image could spoil it if forwarded. Because of this rule, the page does not depend on `joy_who`, which also makes it safe to cache. The planner's version can add one line, "+1 surprise still sealed, it'll join the recap once revealed". Render it only when `who == drawn_by`, and never in the share image.

### 3.2 Which date counts

`HistoryRow.day` is the **kept** date (`picked_at`). A date can be kept in December and happen in January. Use the date it happened, when known:

```
at = COALESCE(h.planned_at, h.picked_at)          -- unix seconds
year = strftime('%Y', at, 'unixepoch', 'localtime')  -- TZ=Asia/Kolkata on Fly
```

Only count rows where `at <= unixepoch()` (it already happened). Report future planned ones separately as "N still to come".

### 3.3 Queries (sketch)

**Recommendation:** run **one query** that returns this year's eligible rows, then compute everything in Rust. There are at most a few hundred rows, so this costs microseconds, is easy to unit-test, and avoids SQL tricks for splitting the comma-separated `tags`. Don't reuse `db::history`. It has `LIMIT 300` and includes skipped and sealed rows.

```sql
-- the one query: this year's dates that happened and that both of us may see
SELECT h.id, h.rating, h.note, h.has_photo, h.drawn_by,
       COALESCE(h.est_cost, a.est_cost) AS est, h.actual_cost AS act,
       a.title, a.setting, a.tags, a.energy, a.duration,
       CASE WHEN a.venue != '' THEN a.venue ELSE a.area END AS place,
       COALESCE(h.planned_at, h.picked_at) AS at
FROM history h JOIN activities a ON a.id = h.activity_id
WHERE h.outcome = 'did_it'
  AND (h.surprise = 0 OR h.revealed = 1)
  AND COALESCE(h.planned_at, h.picked_at) <= unixepoch()
  AND strftime('%Y', COALESCE(h.planned_at, h.picked_at), 'unixepoch', 'localtime') = ?1
ORDER BY at;
```

The same figures as SQL aggregates, for cross-checking in tests or if pure SQL is preferred (`y` is the query above as a CTE):

```sql
-- totals; compare spend only where an actual cost was filled in
SELECT count(*)                                         AS dates,
       sum(act)                                         AS spent_actual,
       sum(CASE WHEN act IS NOT NULL THEN est END)      AS est_for_those,
       sum(est)                                         AS est_all,
       count(act)                                       AS with_actual,
       round(avg(rating), 1)                            AS avg_rating,
       count(rating)                                    AS rated,
       sum(has_photo)                                   AS photos
FROM y;

-- by month (always 12 rows in Rust: fill the gaps with 0)
SELECT CAST(strftime('%m', at, 'unixepoch', 'localtime') AS INTEGER) AS m, count(*) AS n
FROM y GROUP BY m ORDER BY m;

-- favourite vibe (setting: home | local | day_trip | travel)
SELECT setting, count(*) AS n FROM y GROUP BY setting ORDER BY n DESC, setting LIMIT 1;

-- top tags (works because tags are saved trimmed + lowercase; Rust is safer)
SELECT trim(j.value) AS tag, count(*) AS n
FROM y, json_each('["' || replace(y.tags, ',', '","') || '"]') AS j
WHERE trim(j.value) != '' GROUP BY tag ORDER BY n DESC, tag LIMIT 3;

-- highest-rated night: best rating, then most recent; also report how many share it
SELECT id, title, rating, at FROM y WHERE rating IS NOT NULL
ORDER BY rating DESC, at DESC LIMIT 1;

-- photo wall
SELECT id FROM y WHERE has_photo = 1 ORDER BY at;
```

**Computed in Rust:**
- **Streak:** the longest run of consecutive weeks with at least one date. Days are too strict for a date jar, and weeks read nicely: "7 weeks in a row". Build it from distinct `(year, ISO week)` keys of `at` in local time. Also give the current streak if the year is ongoing, and the busiest month.
- **Ties:** with several 5-star nights, show the most recent one and add "and 4 more five-star nights".
- **Money:** "₹42,300 spent, ₹3,100 under what we guessed (on the 18 dates we logged)". Use `est_for_those` vs `spent_actual`, so dates without an actual cost don't skew the comparison. Uses `cfg.money`.
- **Who drew more:** don't include it. It's a turns feature, and it invites score-keeping.

**Performance:** one query, a full scan of `history` joined by primary key. That is under 1 ms for 10³ rows on the shared CPU. No index is needed. If it's ever wanted, an expression index on `COALESCE(planned_at, picked_at)` would do, but it isn't needed at this size.

### 3.4 Page content (server-rendered)

1. Big number: "**23** dates kept in 2026".
2. Months strip: 12 small bars, using CSS custom properties. See the CSP note in section 5.
3. Favourite vibe + top 3 tags, using the existing setting tint tokens.
4. Highest-rated night: title, month, stars, and the note's first line (on the page only, never in the image by default).
5. Money: actual vs estimate.
6. Streak.
7. Photo wall: `<img loading="lazy" decoding="async">` pointing at the existing `/photos/{id}?v=` URLs. Each photo has an **"include in image"** checkbox, **unchecked by default**.
8. "Make our picture" button → share-image preview.

The page shows titles and places as normal, because it is private like the rest of the app. The **share image** gets a stricter data set (section 3.6).

### 3.5 Share image, made on the device

**Pipeline** (main thread, plain `<canvas>`):

1. The server writes a **share-safe JSON island** into the page: `<script type="application/json" id="year-share">`. This is the same pattern as `decide.html`'s `#slips`, and it is not executed, so CSP allows it. It holds only fields cleared for sharing (section 3.6).
2. The user taps **"Make our picture"** (tap 1). JS then:
   - loads fonts explicitly. `document.fonts.ready` only waits for faces the page has *already requested*. Caveat and Fraunces-italic may never have been used on the page, so call `await Promise.all([document.fonts.load('600 96px Fraunces'), document.fonts.load('italic 48px Fraunces'), document.fonts.load('40px Caveat'), document.fonts.load('32px "Instrument Sans"')])` before drawing. Otherwise the canvas silently draws in a fallback font;
   - decodes the chosen photos with `img.decode()` or `createImageBitmap(blob)`, from `/photos/{id}?v=`. They are same-origin, so the canvas is not tainted and `toBlob` works;
   - draws a 1080×1920 (story) or 1080×1350 (feed) canvas. Photos are drawn already scaled to tile size, using `drawImage` with destination width and height;
   - `canvas.toBlob(cb, type, q)`: **PNG when there are no photos** (flat colours compress well, ≈ 100–300 KB), **JPEG q=0.9 when photos are included** (PNG of photos can reach several MB, estimate). Wrap it in `new File([blob], 'our-2026.png', {type})`;
   - shows a preview `<img src=URL.createObjectURL(file)>` in a sheet with a **Share** button. It also checks `navigator.canShare({files:[file]})` now.
3. The user taps **Share** (tap 2). Call `navigator.share({ files: [file] })` **synchronously, first thing in the handler**.

**Why two taps:** `share()` needs *transient user activation*. WebKit only says the activation window is "a short time … a few seconds, maybe", and it is used up by `share()`. Awaiting font loads, photo decodes and `toBlob` inside one tap can push past that window and fail with `NotAllowedError`. The app already solves the same problem for `.ics` by fetching the file before the tap (`static/kept.js` `prepIcs`, `static/jar.js` `icsCache`). The year page follows that pattern.

**Share only `{files}`**, without `text`/`title` alongside. WebKit bug 244408 reports `NotAllowedError` on iOS when image files and text are combined, even though `canShare` returned true. Put the caption in the image itself. On `AbortError` (the user closed the sheet) do nothing.

**Fallbacks**, in order:
1. `canShare({files})` is false (desktop Firefox, older browsers): show the preview with a **Save image** link (`<a download="our-2026.png" href="blob:…">`) and the hint "or long-press the picture to save it".
2. `share()` rejects with something other than `AbortError`: same as 1.
3. No JS: the page is still fully readable. A screenshot is the fallback.

**Memory and size limits (iOS):**
- Canvas area cap ≈ **16,777,216 px** (4096² equivalent) per canvas. Total canvas memory has been reported at ~224–384 MB, varying by device and iOS version (developer reports, not Apple docs). 1080×1920 = 2.07 MP ≈ 8.3 MB, so it is far inside both. **Do not** render at devicePixelRatio 3 (3240×5760 = 18.7 MP, *over* the cap). The 1080-wide output *is* the target resolution for stories.
- After sharing, free memory: `canvas.width = canvas.height = 0`, `URL.revokeObjectURL`, and `bitmap.close()` for each `ImageBitmap`.
- Decode at most ~9 photos for the image (a 3×3 grid). Each decoded 1600 px photo is ~1600×1200×4 ≈ 7.7 MB, so 9 ≈ 70 MB transient. That is acceptable, but `createImageBitmap(blob, {resizeWidth: 360, resizeQuality: 'high'})` cuts it to ~0.5 MB each. Feature-detect the resize options; Safari's support for them is **(verify on device)**.
- **OffscreenCanvas in a worker: not used.** Safari has had OffscreenCanvas 2D and `convertToBlob` since 16.4, but loading web fonts inside a worker (FontFace in workers) is the long-standing gap. The work is ~50 ms of drawing, so a worker brings no benefit.

### 3.6 Privacy defaults for the share image

| Data | On the page | In the share image (default) |
|---|---|---|
| Our names | yes (nav) | **no**. The image says "us". `{a}`/`{b}` in slip titles must be replaced (`cfg.personalize` puts real names in titles such as "watch Neha react") |
| Home areas `{a_area}`/`{b_area}` | yes | **never**. Render titles with a share-safe personalize (areas = None, which already drops `[[…]]` blocks, and names → "us") |
| Venue / area of each date | yes | **off**. One toggle "show places". Even then, never lat/lng |
| Unrevealed surprises | no (excluded from the whole recap) | no |
| Notes | first line of the top night | **no** |
| Photos | yes | **opt-in per photo**, none by default |
| Money | yes | **off by default**, toggle "show what we spent" |
| Ratings, counts, months, vibe, tags, streak | yes | yes |
| App URL / host name | n/a | **no**. Don't print `joy-jar.fly.dev` on the image |

**Photo metadata:** confirmed in `src/photos.rs`. Uploads are decoded, oriented, resized and re-encoded with `JpegEncoder::encode_image(img.to_rgb8())` without passing EXIF, so stored photos already carry **no GPS, camera or time metadata**. A canvas export also never carries EXIF. The share image only goes where the user sends it from the share sheet.

**Uncommitted work note:** the working tree currently has uncommitted changes, including `migrations/0008_smarter_jar.sql`, which adds per-person `cab_fare_0/1` and `vetoes`. If those land, the recap can show "getting there: ₹X in cabs" as a separate line. Keep it out of "spent vs estimate" unless the estimate also included cabs. **Never** show vetoes in the recap: they are per-person and would invite blame.

---

## 4. Feature 12: "Opens instantly, even offline"

### 4.1 Platform facts this design depends on (October 2026)

| Fact | Status | Source |
|---|---|---|
| Service workers + Cache Storage in iOS home-screen web apps | Supported since iOS 11.3. Navigation Preload since 15.4. SW Static Routing listed for 27.0 | firt.dev iOS PWA notes |
| Home-screen app storage is **separate** from Safari's | Yes: "storage shared with browser: no". Login cookies are copied to the installed app since 17.2 | firt.dev |
| iOS 26: every site added to the Home Screen opens as a web app by default (toggle "Open as Web App") | Yes | heise, firt.dev |
| Origin quota | Since Safari/iOS 17: browser apps (and home-screen web apps, which get "the same origin quota and overall quota") **up to 60% of disk** per origin, 80% overall | WebKit "Updates to Storage Policy" |
| Eviction | Best-effort by default, **LRU per origin**. Triggers: overall quota exceeded, system storage pressure, or "a period without user interaction". `navigator.storage.persist()` is granted by heuristics, including "whether the website is opened as a Home Screen Web App" | WebKit storage policy |
| 7-day cap on script-writable storage (ITP) | Applies after 7 days *of Safari use* without interaction. "Web applications added to the home screen are not part of Safari and thus have their own counter of days of use." In practice an installed app that is used never hits it. Reports exist of installed apps losing state after 7 days (WebKit bug 237350), so treat cache loss as normal and design for it | WebKit ITP post (2020), bug 237350 |
| Background Sync / Periodic Sync / Background Fetch on iOS | **Not supported** (caniuse up to iOS 26.5; firt.dev) | caniuse, firt.dev |
| Web Share Level 2 (files) on iOS | Works since iOS 15. Combining files + text has a known `NotAllowedError` bug | Apple forums, WebKit 244408 |
| `Clear-Site-Data` in Safari | caniuse/MDN BCD: `"cache"` and `"storage"` supported from **Safari 17.0** (also iOS). WebKit bug 264852: incomplete in ephemeral sessions | caniuse, WebKit bug |
| What `"storage"` clears | localStorage, sessionStorage, IndexedDB, **service worker registrations** (unregister), FileSystem. Whether WebKit also empties **Cache Storage** under `"storage"` or `"cache"` is **not confirmed**, so the design must not depend on it | MDN Clear-Site-Data |

**Practical meaning:** quotas are not a concern. The whole offline footprint is < 30 MB, against a quota measured in GB. Eviction *is* possible, so offline is "best effort, usually there".

### 4.2 Service worker: where it lives and what it is

- **Serve it at `/sw.js`** with a dedicated axum route, *not* from `/static/sw.js`. A worker's scope can't go above its own path, so `/static/sw.js` could only control `/static/`. Mount it **outside `require_login`**. It holds no secrets, and if an expired session made the worker script request redirect to `/login`, the update would fail. Headers: `Content-Type: text/javascript`, `Cache-Control: no-cache`, `Service-Worker-Allowed` not needed.
- The **build id** (git SHA or `CARGO_PKG_VERSION` + timestamp, injected by `build.rs` as an env var) is put into the worker body (`const V = "…"`) and into every asset URL (`/static/kept.css?v=…`), so each deploy gets a new worker and new asset URLs.
- **`/offline` also lives outside `require_login`** (or as `/static/offline.html?v=<build>`). It holds no personal data. If it sat behind login, a worker update installing after the 30-day cookie expired would follow the 303 to `/login`, and the login page would be stored as the offline page. **Precache must reject any response with `redirected === true` or `!ok`.** `cache.addAll` doesn't check `redirected`, so use explicit `fetch` + check + `put`. Safari also refuses to serve a redirected response to a navigation.
- **Registration** at the end of `base.html`'s deferred script (or in a tiny `static/app.js`): `if ('serviceWorker' in navigator) addEventListener('load', () => navigator.serviceWorker.register('/sw.js'))`. **Don't register on `/login`.** There is nothing useful to cache before login, and it avoids caching the login page.
- About **150–200 lines of plain JS**. No Workbox: it would add a dependency bigger than the logic.

### 4.3 Caching strategy

| Request | Strategy | Cache name | Notes |
|---|---|---|---|
| `/static/*?v=<build>` (CSS, JS, htmx, fonts, icons, manifest) | **Precache on install, cache-first** | `joy-static-<build>` | Old build caches deleted on `activate`. Server sends `Cache-Control: public, max-age=31536000, immutable` **only when `?v=` matches the current build**, otherwise `no-cache` |
| Navigations `GET /`, `GET /history`, `GET /wishlist`, `GET /year/*` | **Network-first, 2 s timeout → cached copy → `/offline`** | `joy-pages` | Cache only if `res.ok && !res.redirected && res.type === 'basic'`. Tag each with the fetch time and viewer (section 4.5) |
| `GET /photos/{id}?v=…` | **Cache-first, runtime, LRU** | `joy-photos` | Cap 25 MB / 80 entries, LRU by last-served time. **Same 7-day cap as pages.** **Pruned by reference:** after each network `/history` or `/year/*` response, the worker deletes photo entries the page no longer links (deleted date, deleted or replaced photo). Otherwise a deleted photo would stay on the phone until LRU eviction, because nothing requests that URL again. A miss offline → placeholder |
| `GET /photos/{id}` **without `?v=`**, or `request.cache === 'reload'` | **Pass through (network only)** | — | `kept.js` refreshes a new upload with `fetch("/photos/"+id, {cache:"reload"})` and then refetches `/history`. Intercepting that would break the "developing…" upload flow |
| `GET /offline` | Precached | `joy-static-<build>` | A static "You're offline. Here's what this phone remembers" page linking to the cached pages, with no personal data |
| Anything with `HX-Request` header, **any non-GET**, `/pick`, `/match`, `/accept/*`, `/history/when`, `*.ics`, `/login`, `/logout`, `/who`, `/sw.js`, `fetch()`es from page JS (non-navigation, e.g. `kept.js` → `/history`) | **Never cached, never answered from cache**: `fetch(event.request)` | — | Handled by an **allowlist**: the worker only acts on the rows above and passes everything else through untouched |

**Why network-first for pages and not stale-while-revalidate:** `/` shows the live turn, tonight's rain and budget, and `/history` shows ratings and surprise state that the *other* phone may have just changed. SWR would show the old version first and then do nothing visible until the next open. Network-first with a short timeout gives fresh pages when online and the cached copy within 2 s when offline or on bad signal. The **Navigation Preload** API (iOS 15.4+) is used so the network request starts while the worker boots. That removes the worker's start-up cost (tens of ms on a phone, an estimate) from the critical path.

**About the 2 s timeout and Fly cold starts:** a cold start can itself take ~2 s. A cold machine plus a 2 s timeout would show the cached copy, and the fresh copy would arrive too late to be used. Two options:
- **(a, recommended)** Show the cached page and let the network response finish in the background. When it arrives, update the cache, and post a message to the page so it shows a small "updated, tap to refresh" chip. The page never swaps content by itself, so nobody gets lost mid-scroll.
- (b) Raise the timeout to 4 s.

With `auto_stop_machines = 'suspend'` the wake becomes "a few hundred ms" (Fly docs), the timeout almost never triggers, and the issue mostly goes away.

### 4.4 Numbers

| Item | Size | How measured |
|---|---|---|
| Fonts (5 × woff2) | 408 KB | `ls -la static/fonts` |
| CSS (6 files) | 118 KB | `ls -la static` |
| App JS (4 files) | 61 KB | 〃 |
| htmx 2.0.4 | 51 KB | 〃 |
| Icons + manifest | 28 KB | 〃 |
| **Precache total** | **≈ 665 KB** uncompressed. The download is smaller if the server gzips CSS/JS (fonts are already compressed). Optional: skip `login.css`/`login.js` (21 KB) | |
| Page cache | 3–4 pages × ~20–60 KB (estimate) ≈ **≤ 250 KB** | |
| Photo cache | **cap 25 MB / 80 photos**. At an estimated 150–400 KB per 1600 px q82 JPEG, 80 photos ≈ 12–32 MB, so the byte cap is what normally applies. LRU eviction on insert | estimate. Measure with `du -sh /data/photos` / count |
| **Total worst case** | **≈ 26 MB**, against a multi-GB quota | |
| First load (cold, never visited) | Unchanged. Registration starts after `load`. The install step downloads ~665 KB once in the background. Most of it is already in the HTTP cache from the page itself, but `cache: 'reload'` precaching may fetch it twice. Use versioned URLs so the precache request is the same URL the page used | |
| Repeat open, online | Static: 0 network requests (today: one conditional request each for every CSS/JS/font, ≈ 6–9 revalidations per page). HTML: 1 request | |
| Repeat open, offline | Cached page in < 100 ms (estimate) | |
| Server impact | **≈ 0, slightly negative**: it no longer answers revalidation requests for static files, so fewer requests wake the machine. Browsers check for a worker update on **every in-scope navigation** (the 24-hour rule only covers functional events and bypassing the HTTP cache). With `Cache-Control: no-cache` that is one tiny `/sw.js` request (≈ 5 KB, or a 304) next to the HTML request it comes with. It never wakes the machine on its own | |

### 4.5 Privacy design for the cache (the important part)

**Threat:** one phone, two people. Cached HTML holds per-viewer data, including the planner's view of an unrevealed surprise. After **"who's holding this phone" switches** from the planner to the partner, the partner must never be shown a cached page made for the planner, whether online, offline or via the back button.

**Facts that make this workable:**
- `joy_who` is HttpOnly, so the worker **can't read it**. The worker has to *learn* the viewer from the server.
- Who can only change **through the server** (`POST /who` sets the cookie). Offline, the form submission fails, so the viewer can't change while offline.
- `POST /who` and `POST /logout` are plain HTML form submissions, which means they are **navigation requests that pass through the worker's `fetch` event**. The worker therefore sees every switch, even with page JS disabled.

**Layered defence (all four layers, cheap):**
1. **Purge in the worker on switch/logout:** in `fetch`, if `request.mode === 'navigate' && request.method === 'POST'` and the path is `/who` or `/logout`, the worker runs `await Promise.all([caches.delete('joy-pages'), caches.delete('joy-photos')])` and *then* forwards the request (`event.respondWith(purge().then(() => fetch(request)))`). This needs no page script and does not depend on Clear-Site-Data.
2. **Viewer tag on every page response:** the server adds `X-Joy-Viewer: <0|1|none>` (plus `X-Joy-Build`) to HTML responses. The worker stores the last-seen viewer in a tiny `joy-meta` cache entry (the worker can't use localStorage). If a network response shows a different viewer than the stored one, it **purges `joy-pages` + `joy-photos` before caching the new page**. This catches a cookie changed by any path other than `/who`, such as clearing cookies or a new login on a phone that was used by the other person.
3. **Serve from cache only if the tag matches:** each cached page carries `X-Joy-Viewer` and `X-Joy-Fetched` (the worker rebuilds the response with these headers when storing it). When answering offline, the worker only uses an entry whose viewer equals the last-known viewer *and* whose age is ≤ 7 days.
4. **HTTP cache and bfcache:** send **`Cache-Control: no-store`** on per-viewer HTML. The Cache API ignores Cache-Control, so the worker can still cache it. `no-store` keeps the page out of the HTTP cache and, in general, out of the back/forward cache. Chrome's newer exception for `no-store` pages still evicts them when cookies change, and a switch changes `joy_who`. Result: there is only *one* store (the worker's) to purge . As an extra layer, on the `POST /who` response (a 303 redirect), send `Clear-Site-Data: "cache"`. That clears the HTTP cache **and, per MDN, may clear the back/forward cache**, so "Back" after switching can't restore the planner's page from memory. **(verify on device)**: if the bfcache restore still happens on iOS, add a `pageshow` handler: `if (e.persisted && document.body.dataset.viewer !== currentViewerFromMeta) location.reload()`, with the viewer written on `<body data-viewer>` by the server. Note: the year page and the photo cache are not viewer-sensitive for *sealed* content (photos of sealed rows return 404 for the partner, so they are never cached for them). The planner's cached photo of a still-sealed date **is** sensitive, which is why `joy-photos` is purged on switch too.

**Logout:**
- The worker purges `joy-pages` + `joy-photos` (layer 1).
- **Recommended** server response to `POST /logout`: `Clear-Site-Data: "cache", "storage"`. Per MDN, `"storage"` unregisters the worker and clears localStorage/IndexedDB. Safari supports both directives since 17.0 per caniuse. Side effects: the worker is reinstalled on next login (≈ 665 KB again), and the `joy.shake` localStorage preference is lost. Both are acceptable for a logout. Don't use `"cookies"`: it would also drop `joy_who`, which is a separate decision (next point).
- **Should logout clear `joy_who`?** Today it doesn't. **Recommend: yes.** Logout means "this phone is no longer ours", and the next person to log in should be asked "who's holding this phone?" (the flow already does this when no cookie exists). Without this, a phone handed to the partner and logged back in would keep the planner's identity and reveal sealed surprises in the live page, not just in the cache.

**Offline readability without the passcode (state it plainly):**
- Cached pages and photos can be read **offline without the passcode**. The 30-day `joy_auth` cookie already lets anyone holding the unlocked phone open the app *online* too, so the offline cache does not weaken today's model much.
- **What protects them is the phone's lock screen and iOS/Android device encryption** (Data Protection on iOS: app data is encrypted while the device is locked). Nothing in joy encrypts them.
- **Mitigations, recommended:**
  - **7-day cap on offline copies.** Older entries are never served and are deleted by the worker on `activate` and on each page load. 7 days covers a trip without signal and stays well inside the 30-day login.
  - **A per-phone "Don't keep offline copies on this phone" switch** on `/who`, the natural place since it is about the device. It is stored where the worker can read it (a `joy-meta` cache entry or IndexedDB). When on, the worker purges and only precaches static assets. The switch is per device and needs no server state.
  - Don't cache `/year/*` offline unless visited, and never cache `.ics` (it contains place and maps links).
- Encrypting the cache with a key derived from the passcode was considered and rejected. It would need the passcode on every offline open (WebCrypto + a PIN prompt). That is a real feature with a real UX cost, for a threat (an unlocked, stolen phone) that the 30-day cookie already exposes.

### 4.6 Offline writes: decision

**Recommendation: read-only offline in v1.**
- iOS has no Background Sync, so a queued write is only sent when the app is opened again *and* online. A rating made on the train might be sent days later, after the partner has rated the same night. Last-write-wins would silently overwrite their rating.
- Most writes are tied to server-side meaning: the drawing logic, peeks, turns, the surprise `accept`, photo processing, reveal. None of these can be done honestly offline.
- **Offline UI:** the worker marks offline responses (`X-Joy-Offline: 1`). `kept.js`/`jar.js` see `navigator.onLine === false` or that header, add `body.is-offline`, disable forms, and show one line: "Offline: showing what this phone saw on Sat 18 Oct. Changes need a connection." htmx requests made offline fail, and htmx's `htmx:sendError` event shows the same line.
- **Possible v2:** queue **only `rating` + `note`** (`POST /history/{id}`, already an idempotent "set" operation) in IndexedDB. Send it on `online` / `visibilitychange`, and drop it if the server's row changed since (send an `If-Unmodified-Since`-style `rated_at`). That is only worth building if offline rating turns out to be a real habit.

---

## 5. Security and privacy headers (recommendations only; the user decides)

Add them once, in one `tower_http::set_header::SetResponseHeaderLayer` (or a small `middleware::map_response`) in `build_app`, so every response gets them, including `/login` and `/static`.

| Header | Recommended value | Why / notes |
|---|---|---|
| `Referrer-Policy` | `no-referrer` | Outbound links to Google Maps, Uber and wa.me (`templates/pick_card.html`) use `rel="noopener"`, which **does not** strip the Referer. With the browser default (`strict-origin-when-cross-origin`) those sites receive `https://joy-jar.fly.dev/`, which tells them the app exists and where. Nothing in joy reads `Referer`, so `no-referrer` breaks nothing. Also add `noreferrer` to those links (`rel="noopener noreferrer"`) as a second layer. Caveat: links that iOS hands to the native Uber/Maps app as universal links send no Referer at all. The header covers the web fallback |
| `Content-Security-Policy` | see below | Blocks injected script. Today Askama auto-escapes, so this is defence in depth |
| `X-Content-Type-Options` | `nosniff` | Stops `/photos/*` or uploads being sniffed as HTML/JS. Free |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=(), payment=(), usb=()` (keep `web-share=(self)`, which is the default) | joy uses none of these. **Safari only partly honours this header** (caniuse shows "partial"/"not supported" depending on the feature). It is cheap and other browsers enforce it. **Don't** block `accelerometer`/`gyroscope`: `jar.js` uses `DeviceMotionEvent` for "shake the jar" |
| `X-Frame-Options` | `DENY` (or CSP `frame-ancestors 'none'`) | Clickjacking on the passcode form. Both, for older engines |
| `Strict-Transport-Security` | `max-age=31536000` | Fly already sets `force_https`. HSTS stops the first plain-HTTP hop. Leave out `preload` (it's a fly.dev subdomain) |
| `Cross-Origin-Opener-Policy` | `same-origin` | Isolates the window from pages opened with `target=_blank`. `noopener` already does this per link |
| `Cache-Control` on HTML | `no-store` | Section 4.5: one cache to purge, and the shared HTTP cache never stores personal pages |
| `Cache-Control` on `/photos/{id}?v=` | **today:** `private, max-age=86400` → **recommend** `private, max-age=31536000, immutable` when `v` matches the file's mtime, else `private, no-cache` | The URL is already versioned by mtime, so a long lifetime is safe and saves revalidation requests (which wake Fly). `private` stays, so no CDN/proxy stores them. A sealed photo is never even linked for the partner, and returns 404 for them. On logout/switch the HTTP cache is cleared by `Clear-Site-Data: "cache"` (section 4.5) |
| `Cache-Control` on `/static/*?v=<build>` | `public, max-age=31536000, immutable` | Today `ServeDir` sends none, which means about 6–9 conditional revalidation requests per page view |
| `Clear-Site-Data` | `/logout`: `"storage"` only. `/who` POST: none | **Changed after launch:** `"cache"` made browsers stall the redirect while clearing (switching looked stuck on phones). The worker purge and `no-store` already cover it; a `pageshow` reload handles bfcache |

**Proposed CSP**, enforced only after a week in `Content-Security-Policy-Report-Only` with no violations. joy has no report endpoint; just check Safari's Web Inspector console on each page.

```
default-src 'none';
script-src 'self';
style-src 'self';
style-src-attr 'unsafe-inline';
img-src 'self' data: blob:;
font-src 'self';
connect-src 'self';
manifest-src 'self';
worker-src 'self';
form-action 'self';
frame-ancestors 'none';
base-uri 'none';
object-src 'none'
```

**What in today's templates blocks a strict CSP** (found by grep):
1. **Inline `<script>` in `templates/history.html:6`**: `document.documentElement.classList.add("k-js")`. Either move it to the top of `kept.js` (it's `defer`, so there may be a flash of the no-JS layout), or keep it and allow its hash: `script-src 'self' 'sha256-2VB5ZxFbXeznYjXzdlRvsuSETp9Kv+tkCEN4V78uTbA='`. That hash was computed for the exact current text. Any whitespace edit changes it. Recommend moving it into a 1-line `static/early.js` loaded *without* `defer` in `<head>`. That avoids both the flash and the hash.
2. **16 `style="--x: …"` attributes** (decide.html 10, history.html 4, pick_card 1, wishlist 1) that set CSS custom properties. They need `style-src-attr 'unsafe-inline'`. A browser that doesn't understand `style-src-attr` falls back to `style-src 'self'` and **drops** them: decorative bubbles stop animating, and the budget bar and month index lose their values. Safari's support for `style-src-attr` couldn't be confirmed from a primary source **(verify on device)**. Safe options:
   - start with `style-src 'self' 'unsafe-inline'` (low real risk: no user HTML is rendered, and Askama escapes);
   - or, later, convert the attributes to `data-*` and set them in JS with `el.style.setProperty`. CSP allows CSSOM writes, which `jar.js`/`kept.js` already do.
3. **htmx 2.0.4** injects a `<style>` for `.htmx-indicator` unless configured otherwise. Add `<meta name="htmx-config" content='{"includeIndicatorStyles":false,"allowEval":false,"allowScriptTags":false}'>` and put the indicator CSS in `tokens.css`. No template uses `hx-on`, `js:` values or `hx-trigger` filter expressions (only `hx-trigger="change"`), so `allowEval:false` is safe.
4. **Data-URI SVG favicon** in `base.html` → needs `img-src data:`. Already in the policy. Or move it to `/static/icons/favicon.svg`.
5. `<script type="application/json" id="slips">` (decide.html) is a data block, not executed, so CSP doesn't apply. The year page's `#year-share` island works the same way.
6. The share image preview uses `blob:` URLs → `img-src blob:`.

---

## 6. Rollout plan

| Phase | What | Size | Ships alone? |
|---|---|---|---|
| **0. Measure + cheap win** | Try `auto_stop_machines = 'suspend'` in `fly.toml`. Measure cold open on the iPhone before/after (stopwatch or Safari Web Inspector → Network, first byte of `/`). Count photos: `du -sh /data/photos` | 1 line | Yes |
| **1. Headers + asset versioning** | Build id from `build.rs`. `?v=` on all `/static` URLs in templates (an Askama global or a `static_url` filter). `immutable` on versioned static. `no-store` on HTML. `nosniff`, `Referrer-Policy`, `X-Frame-Options`, HSTS, `Permissions-Policy`. `rel="noopener noreferrer"`. CSP in **Report-Only** | small | Yes |
| **2. Year in review page** | `/year/{yyyy}`, query + Rust aggregates, `year.html`, `year.css`, link from Kept. No share image yet | medium | Yes |
| **3. Share image** | `year.js`: privacy toggles, two-tap preview → share, fallbacks | medium | Yes |
| **4. Service worker, static only** | `/sw.js` route, precache versioned static + `/offline`, cache-first. **No HTML caching yet**. Proves update/activate on real phones with no privacy risk | small | Yes |
| **5. Offline pages** | Network-first pages with viewer tag + 7-day cap. Purge on `/who` and `/logout` in the worker. `X-Joy-Viewer` header. `Clear-Site-Data` on logout/who. Logout clears `joy_who`. Offline banner. "Don't keep offline copies" switch | medium | Yes, behind a kill switch (below) |
| **6. Photo cache** | Runtime LRU 25 MB | small | Yes |
| **7. CSP enforce** | Switch Report-Only → enforcing after a clean week | 1 line | Yes |
| (later) | Offline queue for rating + note only, if wanted | medium | |

**Kill switch for the worker:** keep a `JOY_SW=off` env var. When set, `/sw.js` serves a worker that deletes all caches and calls `self.registration.unregister()`. This is the standard way to back out a bad worker, because removing the file alone leaves the old worker running on phones.

## 7. Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| A cached planner page shown to the partner after a switch (spoils a surprise) | Low with the 4 layers | Worker purge on `POST /who`, viewer-tag mismatch purge, serve only matching viewer, `Clear-Site-Data: "cache"` + `pageshow` check for bfcache. A test in section 8 checks each one |
| Stale CSS/JS after a deploy | Medium without versioning | Build id in cache names and URLs. Old caches deleted on `activate` |
| Broken worker that can't be removed | Low | Kill switch. `/sw.js` is `no-cache`. Keep the worker small |
| Cached redirect to `/login` served as `/` | Medium if careless | Cache only `ok && !redirected`. Safari also refuses to use redirected responses for navigations from a worker |
| iOS evicts the cache | Real but harmless | Network-first means a fresh page whenever online. Offline is labelled best-effort. Optionally call `navigator.storage.persist()` once from the installed app (granted by heuristics) |
| Share image `NotAllowedError` on iOS | Medium | Two-tap flow, files only, preview + save fallback |
| Wrong fonts in the share image | Medium | Explicit `document.fonts.load` per face before drawing |
| Recap totals leaking a surprise | Low | Exclude unrevealed surprises from every aggregate, for both viewers. Unit test |
| Names/home areas in the share image via `{a}`/`{b_area}` in titles | Medium (the seed uses them) | Share-safe personalize. Unit test with the seed titles |
| Offline cache read on an unlocked phone | Accepted | Device lock, 7-day cap, opt-out switch. Same exposure as the 30-day login today |
| CSP breaks a decorative style in Safari | Medium | Report-Only first. `'unsafe-inline'` styles as a stepping stone |

## 8. Test plan

**Unit tests (Rust, `cargo test`):**
- Year aggregates on a fixture DB: counts, months with gaps, a weekly streak across a year boundary and across ISO week 53, ties for top rating, spend compared only on rows with an actual cost, `planned_at` in next year counted in next year, future dates excluded.
- **Unrevealed surprise excluded** from every number and from the photo list, for viewer 0, viewer 1 and no viewer. A revealed surprise is included.
- Share-safe personalize: seed titles with `{a}`, `{b}`, `[[… {a_area} …]]` contain neither name nor area.

**Route tests** (the working tree now has `src/route_tests.rs` with `build_app`, so use `tower::ServiceExt::oneshot`):
- `/year/2026` returns 200 for both viewers, with identical bodies apart from the planner-only "+1 sealed" line.
- HTML responses carry `X-Joy-Viewer`, `Cache-Control: no-store`, and the security headers. `/sw.js` is reachable **without** the login cookie and has `no-cache`. `POST /logout` has `Clear-Site-Data` and removes both cookies. `POST /who` has no `Clear-Site-Data` (it stalled redirects); bfcache restores reload via `pageshow`.
- Versioned static URL → `immutable`. Unversioned → `no-cache`.

**Browser (desktop Chrome/Safari first, DevTools → Application):**
- Offline toggle: `/`, `/history`, `/wishlist` load from cache. An uncached page shows `/offline`. htmx actions show the offline line and nothing is queued.
- `POST /pick`, `/match`, `/accept`, `.ics`, `/history` fetches from `kept.js`: **never** in Cache Storage (inspect `joy-pages`).
- Photo upload flow (`kept.js` "developing…" → refreshed photo) works with the worker active.
- Deploy a new build id: new worker activates, old `joy-static-*` deleted, new CSS shows on the next open.
- Kill switch: caches empty, worker unregistered.

**Surprise-switch test (do it on each platform):**
1. As person A, keep a **surprise** date with a photo. Open `/history` and `/year/2026` online so they are cached.
2. Switch to B via `/who`. Check Cache Storage: `joy-pages` and `joy-photos` are empty or hold only B-tagged entries.
3. Go offline (airplane mode). Open the app from the Home Screen. B sees only hints or nothing cached, **never the title/place/photo**.
4. Press Back several times (bfcache). Nothing of A's page appears.
5. Log out. In Web Inspector, check the worker is **unregistered** and Cache Storage is empty. That shows whether `Clear-Site-Data` was honoured on a response passed through the worker's `respondWith`. Layer 1 purges anyway. Log in again: the app asks "who's holding this phone?".
6. Delete a kept date's photo as A, then reload `/history` online. Its entry is gone from `joy-photos`.

**On a real iPhone** (iOS 26/27, the installed app, not the Safari tab, because their storage is separate):
- Connect it by cable to a Mac. iPhone: Settings → Apps → Safari → Advanced → **Web Inspector** on. Mac Safari: Settings → Advanced → "Show features for web developers". Then **Develop → [iPhone] → joy** (the home-screen app shows up as its own entry). Use Storage → Cache Storage and Console for the steps above.
- Install fresh from Safari → Share → Add to Home Screen ("Open as Web App" on). Open it once online, then turn on **Airplane Mode**, force-quit, and reopen. Kept must be readable.
- Share image: make the picture with 0 photos and with 3 photos. Share to WhatsApp, to Photos ("Save Image") and to Instagram Stories. Check the fonts are Fraunces/Caveat, not Times. Check there are no names, no areas and no URL in the image.
- Leave the phone unused for 8+ days (or set the 7-day cap to 1 minute in a debug build). Old offline copies must not be served.
- Repeat the core checks on Android Chrome (installed via "Add to Home screen"), where Background Sync and file sharing behave differently.

---

## 9. Sources

- WebKit, *Updates to Storage Policy* (quotas, eviction, persist heuristics): https://webkit.org/blog/14403/updates-to-storage-policy/
- WebKit, *Full Third-Party Cookie Blocking and More* (7-day cap; home-screen apps have their own counter): https://webkit.org/blog/10218/full-third-party-cookie-blocking-and-more/
- WebKit bug 237350, home-screen app storage lost after 7 days report: https://bugs.webkit.org/show_bug.cgi?id=237350
- MDN, *Storage quotas and eviction criteria*: https://developer.mozilla.org/docs/Web/API/Storage_API/Storage_quotas_and_eviction_criteria
- Maximiliano Firtman, *iOS PWA Compatibility* notes (SW since 11.3, Navigation Preload 15.4, storage not shared with browser, cookies copied 17.2, iOS 26 web-app default, no Background Sync): https://firt.dev/notes/pwa-ios/
- heise, *iOS 26: changed web app behaviour on the home screen*: https://heise.de/-10749652
- caniuse, Background Sync: https://caniuse.com/background-sync
- caniuse, Web Share: https://caniuse.com/web-share
- Apple Developer Forums, Web Share Level 2 on iOS 14/15: https://developer.apple.com/forums/thread/133310
- WebKit bug 244408, `share()` NotAllowedError with files + text: https://bugs.webkit.org/show_bug.cgi?id=244408
- WebKit, *The User Activation API* (transient activation, consumed by share): https://webkit.org/blog/13862/the-user-activation-api/
- WebKit bug 222262, user activation lost across fetch/blob in Safari: https://bugs.webkit.org/show_bug.cgi?id=222262
- MDN, `Navigator.share()`: https://developer.mozilla.org/en-US/docs/Web/API/Navigator/share
- MDN, `Clear-Site-Data`: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Clear-Site-Data
- caniuse, Clear-Site-Data `"cache"`: https://caniuse.com/mdn-http_headers_clear-site-data_cache and `"storage"`: https://caniuse.com/mdn-http_headers_clear-site-data_storage
- WebKit bug 264852, Clear-Site-Data in ephemeral sessions: https://bugs.webkit.org/show_bug.cgi?id=264852
- PQINA, *Canvas area exceeds the maximum limit* (16,777,216 px): https://pqina.nl/blog/canvas-area-exceeds-the-maximum-limit/
- PQINA, *Total canvas memory use exceeds the maximum limit* (384 MB on iOS 15): https://www.pqina.nl/blog/total-canvas-memory-use-exceeds-the-maximum-limit/
- MDN, `OffscreenCanvas.convertToBlob()` (Safari 16.4): https://developer.mozilla.org/docs/Web/API/OffscreenCanvas/convertToBlob
- WebKit bug 224178, FontFace in workers for OffscreenCanvas: https://bugs.webkit.org/show_bug.cgi?id=224178
- caniuse, Permissions-Policy header: https://caniuse.com/mdn-http_headers_permissions-policy
- WebKit bug 227349 / CSP style attribute handling in Safari: https://bugs.webkit.org/show_bug.cgi?id=227349
- Fly.io, *Suspend and resume* (resume "a few hundred ms" vs ~2 s cold start): https://fly.io/docs/reference/suspend-resume/
- web.dev, *The service worker lifecycle* (update checks on navigation): https://web.dev/articles/service-worker-lifecycle
- Fly.io, *Autostop/autostart*: https://fly.io/docs/reference/fly-proxy-autostop-autostart/
- Fly community, *Autosuspend is here*: https://community.fly.io/t/autosuspend-is-here-machine-suspension-is-enabled-everywhere/20942

## 10. Open questions / not verified

- Whether WebKit's `Clear-Site-Data` empties **Cache Storage** (the design doesn't rely on it), and whether `"cache"` evicts the **bfcache** on iOS (a `pageshow` fallback is specified).
- Whether WebKit honours `Clear-Site-Data` on a response that passed through the worker (`respondWith(fetch(req))`) for `POST /logout`. Layer 1 does not depend on it. Device test step 5 checks it.
- Safari support for CSP `style-src-attr` (start with `'unsafe-inline'` styles if unsure).
- Exact transient-activation window in WebKit ("a few seconds"). The two-tap flow avoids depending on it.
- `createImageBitmap` resize options in Safari (feature-detect).
- Whether this 256 MB Fly machine is eligible for `suspend` today (it should be, the limit reported is 2 GB). Test in phase 0.
- Average stored photo size (estimated 150–400 KB). Measure on `/data/photos` and adjust the 25 MB cap.
- Firtman lists SW Static Routing for iOS 27.0. It is not needed by this design. I could not find Safari 27 release notes to confirm other iOS 27 changes.

---

## Appendix A: code facts this design is based on (commit 48b7402 + uncommitted working tree, 2026-10-19)

- Routes: every page and POST except `/login` and `/static/*` sits behind `auth::require_login`. htmx requests with an expired session get `HX-Redirect: /login`. The app is now built in `build_app(state)` (working tree).
- Static: `ServeDir::new("static")`, no Cache-Control, no hashed names. Templates link `/static/x.css` unversioned.
- `src/photos.rs`: decode → EXIF orientation → `thumbnail` ≤ 1600 px → `JpegEncoder` q82 from `to_rgb8()`. **EXIF/GPS is not written back**, so stored photos have no metadata.
- `/photos/{id}`: `private, max-age=86400`. URL `?v=<mtime>`. 404 when sealed for the viewer. `kept.js` refetches `/photos/{id}` (no `?v=`) with `cache: "reload"` after upload.
- Auth: `joy_auth` (signed, 30 d, HttpOnly, Lax). `joy_who` (signed, 365 d, HttpOnly). `POST /logout` removes only `joy_auth`. `POST /who` sets `joy_who`, 303 → `/`.
- `sealed_for(row, me) = surprise && !revealed && (me.is_none() || drawn_by != me)`.
- `HistoryRow.day` = kept date (`picked_at`). `db::history` has `LIMIT 300`. `cfg.personalize` fills `{a}`, `{b}`, `{a_area}`, `{b_area}` in slip text.
- Share prior art: `kept.js` `prepIcs` and `jar.js` `icsCache` fetch files ahead so `navigator.share({files})` runs inside the tap.
- Outbound links in `pick_card.html` (Maps, Uber, wa.me): `target="_blank" rel="noopener"`.
- Inline code: 1 inline script (`history.html:6`), 16 `style=` attributes, a data-URI favicon, and htmx 2.0.4's injected indicator style.
- Fly: one shared-cpu-1x 256 MB machine, `auto_stop_machines='stop'`, `min_machines_running=0`, region `sin`, `TZ=Asia/Kolkata`.
