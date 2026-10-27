# Carrier Operations update: status

_Last updated 2026-10-27._

## Where things stand

| Branch | Commit | Pushed | Contents |
|---|---|---|---|
| `master` | `df826c8` | yes | Multi-tenant release merged; README has a roadmap |
| `release/multiple-tenants` | `1629ccb` | yes | Done; kept for reference |
| `release/carrier-operations` | `df826c8` | yes | Release branch for the update; nothing merged into it yet |
| `feature/discord-webhooks` | `2595bae` | yes | Draft: Discord notifications |
| `feature/hauling-ops` | `caa0e15` | yes | Draft: hauling and colonisation operations, **built on top of `feature/discord-webhooks`** |
| `feature/owner-finances` | `d814206` | yes | Draft: runway, weekly balance history, route tritium, low balance alert, **built on top of `feature/hauling-ops`** |

All the roadmap features (see README → Roadmap) will ship together as one big update. Each feature gets its own branch. The plan is to QA them together in a dev environment and then merge them into `release/carrier-operations`. The branches are stacked (webhooks → hauling → finances), because they share `notify.rs`, the webhook kinds and the migration numbers. Merging `feature/owner-finances` brings the other two with it.

**Both features are drafts.** This machine has no Postgres or Docker, so the only checks run were:
- `cargo clippy --all-targets`: no new warnings
- `cargo test -- notify:: discord:: hauling:: finances::`: 21 tests pass
- `npm run check`: clean
- `npm run build`: clean

The database tests (`#[sqlx::test]`) compile but have never run. They need Postgres; CI has it.

## Conventions
- No `Co-Authored-By` trailers in commit messages.
- Commit messages are plain, and commits are marked `Draft:` until they've been tested in dev.
- Code style: comments explain *why*. Migrations are numbered SQL files in `backend/migrations/`. DB tests go through the real router (see `routes/admin_tests.rs` for the shared `user` / `call` / `carrier` helpers).
- sqlx 0.9 only accepts SQL as static strings, never `format!`. `db/operations.rs` uses a `columns!()` macro with `concat!` to keep one column list.

## Feature 1: Discord webhooks (`2595bae`)

Each carrier can have one Discord webhook. The owner chooses which notices it posts: `jump`, `arrival`, `trade_order`, `order_filled`, `low_fuel`, and `operation` (added in the hauling commit).

- **Storage:** the `carrier_webhooks` table (migration `0017`). It is kept off the `carriers` row because carrier pages serve the whole row to every viewer, and the URL is a secret.
- **Change detection (`backend/src/notify.rs`):** compares the carrier row before and after each event, so a repeat changes nothing and isn't announced twice. Events older than 30 minutes are skipped.
- **Delivery (`backend/src/discord.rs`):**
  - Only `discord.com` webhook URLs are accepted (see the `webhook_url` validator).
  - Messages go out through a background queue (256 deep) and handle 429 rate limits.
  - A 404/401/403 from Discord switches the webhook off.
  - `allowed_mentions` is empty, so messages can't ping anyone.
- **Ownership:** a webhook only delivers while the account that set it up still owns the carrier.
- **Routes (`backend/src/routes/webhooks.rs`):** `GET/PUT/DELETE /api/carriers/{callsign}/webhook`, plus `POST …/webhook/test`, which is in the tight credentials rate-limit bucket.
- **Ingest hooks:** `routes/carrier_event.rs` and `routes/market.rs` call `watch_*` before applying an event and `*_changed` after it.
- **UI:** `frontend/src/lib/components/WebhookSettings.svelte`, shown inside the `CarrierSettings` dialog on the Account page.
- **Config:** compose now passes `PUBLIC_ORIGIN` to the backend, which uses it for links in messages.

## Feature 2: Hauling operations (`caa0e15`)

An operation is a named goal on a carrier. Its targets come either from a colonisation construction depot or from the owner's own list.

- **Migration `0018`:**
  - New tables: `construction_depots`, `operations`, `operation_targets`, `hauling_deliveries`.
  - Also widens the `ingest_stats.kind` check constraint to accept `hauling`.
- **Ingest: `POST /api/hauling/event` (`routes/operations.rs`)**
  - Accepts `ColonisationConstructionDepot`, `ColonisationContribution` and `MarketSell`.
  - Needs a user token; the legacy token stores nothing.
  - Deliveries are deduplicated by `(user, kind, market, commodity, time)`.
  - Only sales to carriers the site already tracks are kept.
- **Progress maths:** `backend/src/hauling.rs` (pure and unit-tested). `db::operations::progress` gathers its inputs.
- **Auto-complete:**
  - A depot operation completes when the depot reports `ConstructionComplete`.
  - Any other operation completes when every target has been sold to the carrier.
  - Either way, the `operation` webhook notice is sent.
- **Routes:**
  - Per carrier: `GET/POST /api/carriers/{callsign}/operations`.
  - Per operation: `GET/PATCH/DELETE /api/operations/{id}`.
  - For the editor: `GET /api/depots?q=` and `GET /api/commodities`.
  - Reads follow the carrier's visibility; only the owner can manage.
- **Plugin 1.3.0 (`edmc-plugin/load.py`, `edc/common.py`):**
  - Forwards the three events with `_StarSystem` / `_StationName` added.
  - Skips `MarketSell` when EDMC's `state["StationType"]` says the commander isn't docked at a carrier.
- **UI:**
  - `OperationList.svelte` on the carrier page.
  - `OperationEditor.svelte`, a dialog for creating and editing.
  - The `/operation/[id]` page.
  - The `.meter` progress-bar styles in `global.scss`.
  - In the admin traffic chart, hauling events share the "Carrier & hauling events stored" bar.

### Decisions made without the user's input (revisit if wanted)

1. Anyone who can see the carrier can see its operations; only the owner manages them. Crew roles come later.
2. The leaderboard shows site usernames, not in-game commander names.
3. Only deliveries made after the operation was created count.
4. Sales at normal stations are thrown away.
5. Operation endpoints answer 403 (not 404) to non-owners who try to manage an operation by id. This reveals that the id exists. Low risk, but could be tightened.

## Feature 3: Owner finances (`feature/owner-finances`)

Runway against upkeep, the balance week by week, and tritium for a route, on a new owner-only page `/carrier/{callsign}/finances` (a "Finances" button on the carrier page).

- **Upkeep is entered by the owner.** The journal never reports it: `CarrierStats` and `CarrierFinance` have balances and tax rates only, and the per-service prices published online disagree. The owner copies the figure from the in-game finance screen.
- **Migration `0019`:**
  - `carriers.weekly_upkeep`, which is NULL when not set. It is on the carrier row, so the live feed recomputes runway as the balance changes. The balance was already on that row.
  - `carrier_webhooks.low_balance_weeks`, default 4.
  - A partial index on `carrier_events (carrier, timestamp)` covering the three balance events.
- **Setting upkeep:** `PATCH /api/carriers/{callsign}` accepts `weekly_upkeep` (0 clears it, capped at 1e9).
- **History:** `GET /api/carriers/{callsign}/finances?weeks=` (owner only, default 12, max 52).
  - Balance snapshots come from the `CarrierStats`, `CarrierFinance` and `CarrierBankTransfer` events logged in `carrier_events`.
  - The maths is in `backend/src/finances.rs`, which is pure and unit-tested.
  - Weeks run from one upkeep tick to the next (Thursday 07:00 UTC). Each week's "change" is the balance movement minus the owner's deposits and withdrawals.
  - Repeated events are collapsed, so a resent transfer is only subtracted once.
  - A week whose figure reaches back into earlier weeks with no snapshot is flagged `spans_weeks`.
- **Route tritium:** `GET /api/systems?name=` looks up coordinates on EDSM (signed-in only). `routeFuel()` in `carrier-fuel.ts` reuses the existing per-jump formula. It assumes a straight line, the current jump range and today's cargo, and lets the tank burn down jump by jump.
- **Low balance notice:** a new `low_balance` webhook kind. It fires once when the balance drops below `upkeep × weeks`. `carrier_notices` now takes an `Alerts` struct instead of a bare `low_fuel`.
- **UI:**
  - The finances page.
  - `WeeklyChart.svelte`, which draws bars above and below zero. `BarChart` only stacks positive values.
  - The low balance option in `WebhookSettings`.
  - `formatDay` in `format.ts`.
- **Plugin:** no change. It already forwards the three balance events.
- **Tests:** `routes/finances_tests.rs` (DB; never run), and unit tests in `finances.rs` and `notify.rs`. I also checked the TS maths by hand in Node. There is no frontend test runner.

### Decisions made without the user's input (revisit if wanted)
1. The history and the finances page are owner-only. The current balance and upkeep sit on the carrier row and are served to anyone who can view the carrier; the balance already was before this branch.
2. The upkeep tick is taken to be Thursday 07:00 UTC (community wiki). If it's wrong, week boundaries shift and the "next upkeep" date is off.
3. Runway uses the total carrier balance, not the available balance, because upkeep is taken from the total.
4. The route estimate is a straight line. A plotted route (Spansh) can need a jump or two more, and the page says so.

### TODO
- [ ] **Upkeep from a services table.** The user is gathering the real weekly upkeep for each service in each state: not installed, installed and active, installed but suspended. Once those figures are in:
  - Replace the single upkeep field with a table of services, with a toggle per service for installed / not installed and active / suspended. Upkeep becomes the core cost plus the sum of each service's cost in its state.
  - Prefill the toggles from the journal. `CarrierStats.Crew` (stored in `carriers.crew`) reports each `CrewRole` with `Activated` (installed) and `Enabled` (not suspended), and `CarrierCrewServices` keeps it current. The owner can still override a toggle.
  - Keep a manual override for the total, in case the game's prices change before the table does.
  - The prices go in one place (a backend constant or table), so the low balance notice and the page share them.

## QA checklist (dev environment)
- [ ] Migrations `0017` and `0018` apply to a copy of the live database.
- [ ] `cargo test` passes in full, including `webhooks_tests.rs` and `operations_tests.rs`.
- [x] Journal field names for `ColonisationConstructionDepot`, `ColonisationContribution` and `MarketSell` match the community schemas (jixxed/ed-journal-schemas). Commodity names differ in case between events (`$Aluminium_name;`, `$aluminium_name;`, `aluminium`), but `commodity_key` normalises all of them. A body that fails to parse gets a 422, which the plugin drops rather than retries. Still worth one look at a real journal line during QA.
- [x] EDMC's `state["StationType"]` exists: `monitor.py` sets it from `Docked` and clears it on undock.
- [ ] Webhooks: save a real URL, use "Send test", and confirm a jump request, cancel, arrival, trade order, low fuel and order filled each post once. Delete the webhook in Discord and confirm the site switches it off.
- [ ] Order-filled detection relies on market reads that include their commodity list. A read with an empty list is ignored on purpose.
- [ ] Hauling:
  - [ ] Create a depot operation after docking at a construction site.
  - [ ] Create a manual operation.
  - [ ] Deliver, and check the progress, the leaderboard, auto-complete and the Discord notice.
- [ ] Look over the new UI in a browser, in both themes and on a narrow screen: the webhook section, operation list, editor dialog and operation page.
- [ ] Plugin 1.3.0 installs from the Account page download and keeps existing settings.
- [ ] Finances:
  - [ ] Migration `0019` applies.
  - [ ] Set the upkeep and check the runway against the in-game finance screen.
  - [ ] Open carrier management, deposit, then open it again, and check that the week's change leaves the deposit out.
  - [ ] Estimate tritium to a known system and compare with Spansh.
  - [ ] Lower the balance below the threshold and check that the Discord notice posts once.
- [ ] Confirm the upkeep is really taken at Thursday 07:00 UTC.

## Next on the roadmap
1. **Crew roles:** share an owner-only carrier with other accounts (view access, or managing orders and operations).
2. **Public discovery:** find carriers by system, service or commodity. EDDN ingestion could seed the data.
3. **Easier plugin setup:** pair the plugin with an account without copying a token.

Also worth doing for growth (not code): list the plugin on the EDMC plugins wiki, have a demo carrier on the landing page, and post in carrier and colonisation Discords once the update ships.
