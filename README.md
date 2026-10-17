# ED-FleetCarrier-Dashboard
## Setup
```
# Clone repo and fill in .env info
git clone https://github.com/Saniee/ED-FleetCarrier-Dashboard
cd ED-FleetCarrier-Dashboard
cp .env.example .env
cp ./backend/.env.example ./backend/.env

# After Edits
docker compose --profile prod up -d --build
```
Edit the root .env file to change the postgres database information, `PUBLIC_ORIGIN`, and optionally `TOKEN` / `ALLOW_REGISTRATION`. Edit the backend .env if not using docker to publish, if wanted to be safe, edit it also even if publishing with docker compose.

Edit the `compose.yaml` file if needed. (Generally can be left untouched.)

`cp compose.override.example.yaml compose.override.yaml` to edit the published ports.

## Accounts and ingest tokens
1. Open the dashboard, go to **Login** and register. Once everyone who needs an account has one, close registration from **Admin → Settings** (or set `ALLOW_REGISTRATION=0` and restart the backend).
2. On the **Account** page create an API token. It is shown once; only a hash is stored.
3. Download the EDMC plugin from your **Account** page (the backend serves the copy from this repo's `edmc-plugin/`, built in), unzip it into EDMC's plugins folder and restart EDMC. In EDMC, open the plugin's settings tab and enter the API URL and that token. The plugin sends it in the `X-Ingest-Token` header.
4. The first `CarrierStats` event from a token claims an unowned carrier (open Carrier Management in game). That is the only way to claim one. Owners choose its visibility (public, link only, owner only) and whether it is their squadron's carrier.

Journal events are not signed, so the server cannot prove who really owns a carrier. If someone claims a carrier that isn't theirs, release it directly in the database and let the real owner's plugin claim it again:

```
docker compose exec db psql -U <user> -d <db> -c "UPDATE carriers SET owner_id = NULL WHERE upper(callsign) = 'ABC-123';"
```

| Variable | Default | Meaning |
|---|---|---|
| `TOKEN` | empty | Optional legacy shared ingest secret. Per-user tokens work without it; empty disables the legacy token. |
| `ALLOW_REGISTRATION` | `1` | `0` closes `POST /api/auth/register`. The default only: the admin dashboard's setting wins while one is set. |
| `RATE_LIMIT` | `1` | Per-IP request limits: login/register 5 burst then 1 per 6 s, ingest 60 burst then 10/s, other API calls 60 burst then 20/s (over the limit: 429). `0` disables. |
| `ALLOW_ANON_INGEST` | off | `1` accepts ingest with no token at all. Dev only. `scripts/reseed.ps1` takes a token instead (`-Token`, or `$env:ED_INGEST_TOKEN`). |
| `DEV_CONSOLE` | off | `1` serves an API test page at `/dev`. Dev only. |
| `BIND_ADDR` | `0.0.0.0:8080` | Address the backend listens on. |

Rate limits key on the client IP the reverse proxy reports: the last `X-Forwarded-For` entry (the one the proxy itself added, so client-supplied entries are ignored), then `X-Real-IP`. The backend must sit behind that proxy (compose binds it to loopback); exposed directly, those headers can be forged.

Migrations run automatically when the backend starts.

### Admins
Admins are made from the server's shell, never from the web, so registering a particular name can't make anyone an admin:

```
docker compose exec backend backend admin grant <username>
docker compose exec backend backend admin revoke <username>
docker compose exec backend backend admin list
```

Outside Docker, run the same from `backend/` with `cargo run -- admin ...`. Each change is written to the `admin_actions` audit log. A disabled account can't log in (403), its sessions stop working, and its ingest tokens are rejected.

Admins manage accounts under `/api/admin/users`: search and inspect accounts, disable or re-enable them (with a reason), sign them out everywhere, revoke their tokens, change roles, and delete them. A disabled account's carriers are hidden from everyone but admins until it is re-enabled; deleting an account leaves its carriers unowned, history intact. An admin can't disable, demote or delete their own account, and the last active admin can't be removed.

Admins also get **Traffic** (`/api/admin/stats?days=`): plugin uploads per day by outcome (stored, ignored, refused), the accounts and carriers sending the most, the biggest carrier histories, and table sizes. Uploads are counted from migration 0016 on. A carrier's history can be trimmed to events after a given day from its dialog.

Carriers are moderated under `/api/admin/carriers`: list every carrier whatever its visibility, give one to another account, release it (the real owner's plugin claims it again with its next CarrierStats), hide it, or delete it with its history. A hidden carrier leaves the carrier list and answers 404 to everyone but its owner, who sees the reason, and admins; the owner's visibility setting can't undo it. Deleting is for clearing bad data such as an impostor holding a callsign: the next event about the carrier creates it again. Admins still can't open an owner-only carrier's page.

The dashboard's `/admin` page does the same from the browser, plus **Settings** (`/api/admin/settings`: open or close registration, or go back to `ALLOW_REGISTRATION`) and the **Audit log** (`/api/admin/audit?page=&user=`), newest first and filterable by account.

### Tests
`cargo test` in `backend/` includes tests that need Postgres: each creates and drops its own temporary database through `DATABASE_URL` (from `backend/.env`), so that role needs permission to create databases. With the compose database running, the default user has it.

# Extending Functionality
## Data Schema
Data schema and all information from the endpoints can be found in the source code.
Mainly:
- [journal_definitions.rs](backend/src/journal_definitions.rs) - Contains fully defined ED journal json schema, specific to Market Events and Carrier* events.
- [carrier.ts](frontend/src/lib/types/carrier.ts) - Contains the full list of carrier table fields with theyre appropriate types.
- [market.ts](frontend/src/lib/types/market.ts) - Contains the definition of the Market event and the secondary commodities schema from Market.json

## Multi-tenant Setup
Heavily WIP on the [release/multiple-tenants](https://github.com/Saniee/ED-FleetCarrier-Dashboard/tree/release/multiple-tenants) branch.

The database already supports multiple carriers, and the backend has endpoints for fetching via `carrier_id`, ~~however there are is no handling for~~:
- [x] User Registration / Login | Issue #1
    - [x] Per User Api Tokens
- [x] Paginated list of Carriers in the database. | Issue #2