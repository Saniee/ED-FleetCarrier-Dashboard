-- Administration: an admin role, disabled accounts, an audit log of admin
-- actions and runtime settings.
--
-- `role` is text rather than a flag so another role (a moderator) only means
-- widening the CHECK.
ALTER TABLE users
    ADD role            text NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin')),
    ADD disabled_at     timestamptz,
    ADD disabled_reason text;

-- Every admin action, written in the same transaction as the action itself.
-- `admin_id` is NULL for the `backend admin` command line, which runs as the
-- server operator. `target_id` has no foreign key so entries about a deleted
-- user survive it.
CREATE TABLE admin_actions (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    admin_id   bigint REFERENCES users(id) ON DELETE SET NULL,
    action     text NOT NULL,
    target_id  bigint,
    detail     jsonb NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX admin_actions_created_idx ON admin_actions (created_at DESC);

-- Settings changed from the admin dashboard. A stored value wins over the
-- matching environment variable; with none stored, the variable applies.
CREATE TABLE app_settings (
    key   text PRIMARY KEY,
    value jsonb NOT NULL
);
