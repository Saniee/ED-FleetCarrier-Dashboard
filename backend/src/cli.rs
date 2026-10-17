//! `backend admin ...`: role management from the server's shell, for making the
//! first admin and for recovery. Whoever can run this already controls the
//! server, so it skips the dashboard's safeguards (it may remove the last
//! admin) but still writes the audit log.

use serde_json::json;
use sqlx::PgPool;

use crate::db::{
    admin,
    users::{ROLE_ADMIN, ROLE_USER},
};

const USAGE: &str = "usage: backend admin list
       backend admin grant <username>
       backend admin revoke <username>";

/// Run an `admin` subcommand. `args` are the words after `admin`.
pub async fn admin(pool: &PgPool, args: &[String]) -> Result<(), String> {
    match args {
        [cmd] if cmd == "list" => {
            let admins = admin::list_admins(pool).await.map_err(|e| e.to_string())?;
            if admins.is_empty() {
                println!("No admins. Make one with: backend admin grant <username>");
            }
            for user in admins {
                println!("{}\t{}", user.id, user.username);
            }
            Ok(())
        }
        [cmd, username] if cmd == "grant" || cmd == "revoke" => {
            let (role, action) = if cmd == "grant" {
                (ROLE_ADMIN, "user.grant_admin")
            } else {
                (ROLE_USER, "user.revoke_admin")
            };
            let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
            let user = admin::set_role_by_username(&mut tx, username, role)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("no user named {username:?}"))?;
            admin::record(&mut tx, None, action, Some(user.id), json!({ "via": "cli" }))
                .await
                .map_err(|e| e.to_string())?;
            tx.commit().await.map_err(|e| e.to_string())?;

            println!("{} is now {role}.", user.username);
            if role == ROLE_USER && admin::list_admins(pool).await.is_ok_and(|a| a.is_empty()) {
                println!("Note: there are no admins left.");
            }
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}
