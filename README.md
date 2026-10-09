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
1. Open the dashboard, go to **Login** and register. Once everyone who needs an account has one, set `ALLOW_REGISTRATION=0` and restart the backend to close registration.
2. On the **Account** page create an API token. It is shown once; only a hash is stored.
3. In EDMC, open the plugin's settings tab and enter the API URL and that token. The plugin sends it in the `X-Ingest-Token` header.
4. The first `CarrierStats` event from a token claims an unowned carrier. Owners choose its visibility (public, link only, owner only) and whether it is their squadron's carrier.

| Variable | Default | Meaning |
|---|---|---|
| `TOKEN` | empty | Optional legacy shared ingest secret. Per-user tokens work without it; empty disables the legacy token. |
| `ALLOW_REGISTRATION` | `1` | `0` closes `POST /api/auth/register`. |
| `RATE_LIMIT` | `1` | Per-IP request limits: login/register 5 burst then 1 per 6 s, ingest 60 burst then 10/s, other API calls 60 burst then 20/s (over the limit: 429). `0` disables. |
| `ALLOW_ANON_INGEST` | off | `1` accepts ingest with no token at all. Dev only (e.g. `scripts/reseed.ps1`). |
| `DEV_CONSOLE` | off | `1` serves an API test page at `/dev`. Dev only. |

Rate limits key on the client IP from `X-Forwarded-For` / `X-Real-IP`, so the backend must sit behind the reverse proxy (compose binds it to loopback); exposed directly, those headers can be forged.

Migrations run automatically when the backend starts.

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