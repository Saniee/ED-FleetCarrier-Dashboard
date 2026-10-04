-- Resolved body names, keyed by the journal's own identifiers.
--
-- `CarrierLocation` reports only a numeric `BodyID`, never a body name, so the
-- name has to be looked up somewhere. EDSM's system endpoint returns bodies keyed
-- by that same `BodyID`, so it is asked once per system and the answers are
-- remembered here — a body never changes its name, so a second lookup never
-- happens.
--
-- `name` is nullable on purpose: a row with a NULL name records a lookup that
-- came back empty, so a system EDSM has never seen is not re-fetched on every
-- jump through it.

CREATE TABLE body_names (
    system_address bigint NOT NULL,
    body_id        bigint NOT NULL,
    -- NULL = EDSM was asked and did not have this body.
    name           text,
    -- EDSM's `type` (Star, Planet, ...); matches the journal's `BodyType`.
    body_type      text,
    resolved_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (system_address, body_id)
);
