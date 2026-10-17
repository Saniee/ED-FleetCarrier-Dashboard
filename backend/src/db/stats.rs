//! Traffic counters (`ingest_stats`) and the admin overview built from them and
//! from Postgres' own size statistics. Days are UTC.

use serde_json::Value;
use sqlx::PgPool;

#[derive(Clone, Copy)]
pub enum Kind {
    Carrier,
    Market,
}

#[derive(Clone, Copy)]
pub enum Outcome {
    Stored,
    Ignored,
    Refused,
}

/// Count one ingest request. `user_id` 0 is the legacy token or anonymous
/// ingest.
pub async fn count(pool: &PgPool, user_id: i64, kind: Kind, outcome: Outcome) -> sqlx::Result<()> {
    let kind = match kind {
        Kind::Carrier => "carrier",
        Kind::Market => "market",
    };
    let outcome = match outcome {
        Outcome::Stored => "stored",
        Outcome::Ignored => "ignored",
        Outcome::Refused => "refused",
    };
    sqlx::query(
        "INSERT INTO ingest_stats (day, user_id, kind, outcome, count)
         VALUES ((now() AT TIME ZONE 'UTC')::date, $1, $2, $3, 1)
         ON CONFLICT (day, user_id, kind, outcome) DO UPDATE SET count = ingest_stats.count + 1",
    )
    .bind(user_id)
    .bind(kind)
    .bind(outcome)
    .execute(pool)
    .await?;
    Ok(())
}

/// The admin overview for the last `days` days (today included): storage,
/// ingest per day and per account, the biggest carriers and account activity.
/// `since.at` is the range's first UTC midnight, so timestamps are cut on UTC
/// days whatever the database's time zone.
pub async fn overview(pool: &PgPool, days: i32) -> sqlx::Result<Value> {
    sqlx::query_scalar(
        "WITH since AS (SELECT d AS day, d::timestamp AT TIME ZONE 'UTC' AS at
                        FROM (SELECT (now() AT TIME ZONE 'UTC')::date - ($1 - 1) AS d) s)
         SELECT jsonb_build_object(
           'days', $1,
           'since', (SELECT day FROM since),
           'database_bytes', pg_database_size(current_database()),
           'tables', (SELECT coalesce(jsonb_agg(jsonb_build_object(
                   'name', relname, 'rows', n_live_tup,
                   'bytes', pg_total_relation_size(relid))
                 ORDER BY pg_total_relation_size(relid) DESC), '[]')
               FROM pg_stat_user_tables WHERE relname <> '_sqlx_migrations'),
           'ingest_daily', (SELECT coalesce(jsonb_agg(jsonb_build_object(
                   'day', day, 'kind', kind, 'outcome', outcome, 'count', n) ORDER BY day), '[]')
               FROM (SELECT day, kind, outcome, sum(count) AS n FROM ingest_stats
                     WHERE day >= (SELECT day FROM since) GROUP BY day, kind, outcome) d),
           'reporters', (SELECT coalesce(jsonb_agg(jsonb_build_object(
                   'user_id', r.user_id, 'username', u.username,
                   'stored', r.stored, 'ignored', r.ignored, 'refused', r.refused,
                   'last_day', r.last_day)
                 ORDER BY r.stored + r.ignored + r.refused DESC), '[]')
               FROM (SELECT user_id,
                            coalesce(sum(count) FILTER (WHERE outcome = 'stored'), 0) AS stored,
                            coalesce(sum(count) FILTER (WHERE outcome = 'ignored'), 0) AS ignored,
                            coalesce(sum(count) FILTER (WHERE outcome = 'refused'), 0) AS refused,
                            max(day) AS last_day
                     FROM ingest_stats WHERE day >= (SELECT day FROM since)
                     GROUP BY user_id
                     ORDER BY sum(count) DESC LIMIT 15) r
               LEFT JOIN users u ON u.id = r.user_id),
           'carriers', (SELECT coalesce(jsonb_agg(jsonb_build_object(
                   'carrier_id', c.carrier_id, 'callsign', c.callsign, 'name', c.name,
                   'events', e.events, 'recent', e.recent, 'bytes', e.bytes,
                   'last_received', e.last_received)
                 ORDER BY e.events DESC), '[]')
               FROM (SELECT carrier,
                            count(*) AS events,
                            count(*) FILTER (WHERE received_at >= (SELECT at FROM since)) AS recent,
                            sum(pg_column_size(json_data)) AS bytes,
                            max(received_at) AS last_received
                     FROM carrier_events WHERE carrier IS NOT NULL
                     GROUP BY carrier ORDER BY count(*) DESC LIMIT 15) e
               JOIN carriers c ON c.carrier_id = e.carrier),
           'accounts', jsonb_build_object(
               'total', (SELECT count(*) FROM users),
               'admins', (SELECT count(*) FROM users WHERE role = 'admin'),
               'disabled', (SELECT count(*) FROM users WHERE disabled_at IS NOT NULL),
               'reporting', (SELECT count(DISTINCT user_id) FROM ingest_stats
                             WHERE user_id <> 0 AND day >= (SELECT day FROM since)),
               'signed_in', (SELECT count(DISTINCT user_id) FROM sessions
                             WHERE created_at >= (SELECT at FROM since)),
               'live_sessions', (SELECT count(*) FROM sessions WHERE expires_at > now()),
               'signups_daily', (SELECT coalesce(jsonb_agg(jsonb_build_object('day', day, 'count', n) ORDER BY day), '[]')
                   FROM (SELECT (created_at AT TIME ZONE 'UTC')::date AS day, count(*) AS n FROM users
                         WHERE created_at >= (SELECT at FROM since) GROUP BY 1) s)),
           'carrier_counts', jsonb_build_object(
               'total', (SELECT count(*) FROM carriers),
               'owned', (SELECT count(*) FROM carriers WHERE owner_id IS NOT NULL),
               'hidden', (SELECT count(*) FROM carriers WHERE moderation_hidden_at IS NOT NULL),
               'with_market', (SELECT count(*) FROM carrier_markets)))",
    )
    .bind(days)
    .fetch_one(pool)
    .await
}
