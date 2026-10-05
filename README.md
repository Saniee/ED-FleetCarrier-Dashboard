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
Edit the root .env file to change the TOKEN, and postgress database information. Edit the backend .env if not using docker to publish, if wanted to be safe, edit it also even if publishing with docker compose.

Edit the `compose.yaml` file if needed. (Generally can be left untouched.)

`cp compose.override.example.yaml compose.override.yaml` to edit the published ports.

# Extending Functionality
## Data Schema
Data schema and all information from the endpoints can be found in the source code.
Mainly:
- [journal_definitions.rs](backend/src/journal_definitions.rs) - Contains fully defined ED journal json schema, specific to Market Events and Carrier* events.
- [carrier.ts](frontend/src/lib/types/carrier.ts) - Contains the full list of carrier table fields with theyre appropriate types.
- [market.ts](frontend/src/lib/types/market.ts) - Contains the definition of the Market event and the secondary commodities schema from Market.json

## Multi-tenant Setup
Heavily WIP on the [release/multiple-tenants](https://github.com/Saniee/ED-FleetCarrier-Dashboard/tree/release/multiple-tenants) branch.

The database already supports multiple carriers, and the backend has endpoints for fetching via `carrier_id`, however there are is no handling for:
- [ ] User Registration / Login | Issue #1
    - [ ] Per User Api Tokens
- [ ] Paginated list of Carriers in the database. | Issue #2