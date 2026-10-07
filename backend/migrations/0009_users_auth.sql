-- Multi-tenancy: users, their login sessions, their ingest API tokens, and
-- carrier ownership.
--
-- Tokens (session and API) are random secrets handed to the client once; only
-- their SHA-256 is stored, so a database leak does not leak usable tokens.
-- Passwords are argon2 PHC strings.

CREATE TABLE users (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username      text NOT NULL,
    password_hash text NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);

-- Usernames are unique regardless of case.
CREATE UNIQUE INDEX users_username_lower_idx ON users (lower(username));

-- Timed session tokens, used by the dashboard after login.
CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY,
    user_id    bigint NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);

CREATE INDEX sessions_user_idx ON sessions (user_id);
CREATE INDEX sessions_expires_idx ON sessions (expires_at);

-- Long-lived per-user tokens for the EDMC plugin (X-Ingest-Token header).
CREATE TABLE api_tokens (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id      bigint NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         text NOT NULL,
    token_hash   bytea NOT NULL UNIQUE,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz
);

CREATE INDEX api_tokens_user_idx ON api_tokens (user_id);

-- Ownership. Existing carriers stay unowned (NULL) until claimed. A private
-- carrier is left out of public discovery but still reachable by its callsign.
ALTER TABLE carriers
    ADD owner_id   bigint REFERENCES users(id) ON DELETE SET NULL,
    ADD is_private boolean NOT NULL DEFAULT false;

CREATE INDEX carriers_owner_idx ON carriers (owner_id);
