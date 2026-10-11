//! PostgreSQL repositories. All queries are runtime-checked so the crate
//! builds without a live database; integration tests cover the SQL.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, Utc};
use hollowrite_game_core::character::Remnant;
use hollowrite_game_core::world::SaveRequest;
use hollowrite_game_core::{CharacterData, Content, Pos};
use hollowrite_protocol::{Appearance, Archetype, CharacterSummary, Equipment};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use uuid::Uuid;

pub use sqlx::migrate::MigrateError;

pub const MAX_CHARACTERS_PER_ACCOUNT: i64 = 4;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("already exists")]
    Conflict,
    #[error("not found")]
    NotFound,
    #[error("limit reached")]
    Limit,
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error("corrupt row: {0}")]
    Corrupt(String),
}

pub type DbResult<T> = Result<T, DbError>;

fn map_unique(e: sqlx::Error) -> DbError {
    match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => DbError::Conflict,
        _ => DbError::Sql(e),
    }
}

#[derive(Debug, Clone)]
pub struct Account {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub failed_logins: i32,
    pub locked_until: Option<DateTime<Utc>>,
    pub suspended_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub is_admin: bool,
}

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../db/migrations");

impl Db {
    pub async fn connect(url: &str, max_connections: u32) -> DbResult<Db> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await?;
        Ok(Db { pool })
    }

    pub async fn migrate(&self) -> Result<(), MigrateError> {
        MIGRATOR.run(&self.pool).await
    }

    pub async fn ping(&self) -> DbResult<()> {
        sqlx::query("select 1").execute(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ---- accounts ----------------------------------------------------

    pub async fn create_account(&self, username: &str, password_hash: &str) -> DbResult<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query("insert into accounts (id, username, password_hash) values ($1, $2, $3)")
            .bind(id)
            .bind(username)
            .bind(password_hash)
            .execute(&self.pool)
            .await
            .map_err(map_unique)?;
        Ok(id)
    }

    pub async fn account_by_name(&self, username: &str) -> DbResult<Option<Account>> {
        let row = sqlx::query(
            "select id, username, password_hash, is_admin, failed_logins, locked_until, suspended_until
             from accounts where lower(username) = lower($1)",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| Account {
            id: r.get("id"),
            username: r.get("username"),
            password_hash: r.get("password_hash"),
            is_admin: r.get("is_admin"),
            failed_logins: r.get("failed_logins"),
            locked_until: r.get("locked_until"),
            suspended_until: r.get("suspended_until"),
        }))
    }

    pub async fn account_id_by_name(&self, username: &str) -> DbResult<Option<Uuid>> {
        Ok(sqlx::query_scalar("select id from accounts where lower(username) = lower($1)")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// Records a failed login and locks the account for `lock` once
    /// `threshold` consecutive failures have accumulated.
    pub async fn login_failed(&self, account: Uuid, threshold: i32, lock: Duration) -> DbResult<()> {
        sqlx::query(
            "update accounts set failed_logins = failed_logins + 1,
                 locked_until = case when failed_logins + 1 >= $2 then now() + $3 else locked_until end
             where id = $1",
        )
        .bind(account)
        .bind(threshold)
        .bind(lock)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn login_succeeded(&self, account: Uuid) -> DbResult<()> {
        sqlx::query("update accounts set failed_logins = 0, locked_until = null where id = $1")
            .bind(account)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_password_hash(&self, account: Uuid, hash: &str) -> DbResult<()> {
        sqlx::query("update accounts set password_hash = $2 where id = $1")
            .bind(account)
            .bind(hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---- sessions ----------------------------------------------------

    pub async fn create_session(&self, account: Uuid, client: &str, ttl: Duration) -> DbResult<String> {
        let token = new_token();
        sqlx::query(
            "insert into sessions (id, account_id, token_hash, client, expires_at) values ($1, $2, $3, $4, now() + $5)",
        )
        .bind(Uuid::new_v4())
        .bind(account)
        .bind(hash_token(&token))
        .bind(client)
        .bind(ttl)
        .execute(&self.pool)
        .await?;
        Ok(token)
    }

    /// Resolves a bearer token to a live, unsuspended account session.
    pub async fn session(&self, token: &str) -> DbResult<Option<SessionInfo>> {
        let row = sqlx::query(
            "update sessions s set last_seen_at = now()
             from accounts a
             where s.token_hash = $1 and a.id = s.account_id
               and s.revoked_at is null and s.expires_at > now()
               and (a.suspended_until is null or a.suspended_until < now())
             returning s.id as session_id, a.id as account_id, a.username, a.is_admin",
        )
        .bind(hash_token(token))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| SessionInfo {
            session_id: r.get("session_id"),
            account_id: r.get("account_id"),
            username: r.get("username"),
            is_admin: r.get("is_admin"),
        }))
    }

    pub async fn revoke_session(&self, token: &str) -> DbResult<bool> {
        let n = sqlx::query("update sessions set revoked_at = now() where token_hash = $1 and revoked_at is null")
            .bind(hash_token(token))
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(n > 0)
    }

    pub async fn revoke_account_sessions(&self, account: Uuid) -> DbResult<u64> {
        Ok(sqlx::query("update sessions set revoked_at = now() where account_id = $1 and revoked_at is null")
            .bind(account)
            .execute(&self.pool)
            .await?
            .rows_affected())
    }

    pub async fn purge_expired_sessions(&self) -> DbResult<u64> {
        Ok(sqlx::query("delete from sessions where expires_at < now() - interval '7 days'")
            .execute(&self.pool)
            .await?
            .rows_affected())
    }

    // ---- characters --------------------------------------------------

    pub async fn list_characters(&self, content: &Content, account: Uuid) -> DbResult<Vec<CharacterSummary>> {
        let rows = sqlx::query(
            "select id, name, archetype, sigil, hue, checkpoint,
                    vitality + endurance + might + finesse + occult as attr_sum
             from characters where account_id = $1 order by created_at",
        )
        .bind(account)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let archetype = parse_archetype(r.get("archetype"))?;
                let sum: i32 = r.get("attr_sum");
                Ok(CharacterSummary {
                    id: r.get::<Uuid, _>("id").to_string(),
                    name: r.get("name"),
                    archetype,
                    level: (sum as u32).saturating_sub(content.archetype(archetype).attrs.iter().sum()) + 1,
                    look: Appearance { sigil: r.get::<i16, _>("sigil") as u8, hue: r.get::<i16, _>("hue") as u8 },
                    location: r.get("checkpoint"),
                })
            })
            .collect()
    }

    /// Inserts a freshly created character. The per-account limit is
    /// enforced inside the transaction with a row lock on the account.
    pub async fn insert_character(&self, c: &CharacterData) -> DbResult<()> {
        let account = parse_uuid(&c.account_id)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("select id from accounts where id = $1 for update").bind(account).fetch_one(&mut *tx).await?;
        let count: i64 = sqlx::query_scalar("select count(*) from characters where account_id = $1")
            .bind(account)
            .fetch_one(&mut *tx)
            .await?;
        if count >= MAX_CHARACTERS_PER_ACCOUNT {
            return Err(DbError::Limit);
        }
        sqlx::query(
            "insert into characters (id, account_id, name, archetype, sigil, hue,
                vitality, endurance, might, finesse, occult, echoes, hp, draughts, draughts_max,
                pos_x, pos_y, checkpoint, equip_main, equip_off, equip_armor, equip_spell1, equip_spell2, version)
             values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)",
        )
        .bind(parse_uuid(&c.id)?)
        .bind(account)
        .bind(&c.name)
        .bind(c.archetype.id())
        .bind(c.look.sigil as i16)
        .bind(c.look.hue as i16)
        .bind(c.attrs[0] as i32)
        .bind(c.attrs[1] as i32)
        .bind(c.attrs[2] as i32)
        .bind(c.attrs[3] as i32)
        .bind(c.attrs[4] as i32)
        .bind(c.echoes as i64)
        .bind(c.hp as i32)
        .bind(c.draughts as i16)
        .bind(c.draughts_max as i16)
        .bind(c.pos.x)
        .bind(c.pos.y)
        .bind(&c.checkpoint)
        .bind(&c.equipment.main)
        .bind(&c.equipment.off)
        .bind(&c.equipment.armor)
        .bind(&c.equipment.spell1)
        .bind(&c.equipment.spell2)
        .bind(c.version)
        .execute(&mut *tx)
        .await
        .map_err(map_unique)?;
        write_children(&mut tx, parse_uuid(&c.id)?, c).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn load_character(&self, account: Uuid, id: &str) -> DbResult<Option<CharacterData>> {
        let Ok(cid) = Uuid::parse_str(id) else { return Ok(None) };
        let Some(r) = sqlx::query("select * from characters where id = $1 and account_id = $2")
            .bind(cid)
            .bind(account)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };
        let items: Vec<(String, i32)> = sqlx::query_as("select item, qty from character_items where character_id = $1")
            .bind(cid)
            .fetch_all(&self.pool)
            .await?;
        let flags: Vec<String> = sqlx::query_scalar("select flag from character_flags where character_id = $1")
            .bind(cid)
            .fetch_all(&self.pool)
            .await?;
        let lanterns: Vec<String> =
            sqlx::query_scalar("select lantern from character_lanterns where character_id = $1")
                .bind(cid)
                .fetch_all(&self.pool)
                .await?;
        let remnant = match (
            r.get::<Option<i32>, _>("remnant_x"),
            r.get::<Option<i32>, _>("remnant_y"),
            r.get::<Option<i64>, _>("remnant_echoes"),
        ) {
            (Some(x), Some(y), Some(a)) => Some(Remnant { pos: Pos::new(x, y), amount: a as u64 }),
            _ => None,
        };
        Ok(Some(CharacterData {
            id: cid.to_string(),
            account_id: account.to_string(),
            name: r.get("name"),
            archetype: parse_archetype(r.get("archetype"))?,
            look: Appearance { sigil: r.get::<i16, _>("sigil") as u8, hue: r.get::<i16, _>("hue") as u8 },
            attrs: [
                r.get::<i32, _>("vitality") as u32,
                r.get::<i32, _>("endurance") as u32,
                r.get::<i32, _>("might") as u32,
                r.get::<i32, _>("finesse") as u32,
                r.get::<i32, _>("occult") as u32,
            ],
            echoes: r.get::<i64, _>("echoes") as u64,
            hp: r.get::<i32, _>("hp") as u32,
            draughts: r.get::<i16, _>("draughts") as u8,
            draughts_max: r.get::<i16, _>("draughts_max") as u8,
            pos: Pos::new(r.get("pos_x"), r.get("pos_y")),
            checkpoint: r.get("checkpoint"),
            lanterns: lanterns.into_iter().collect::<BTreeSet<_>>(),
            flags: flags.into_iter().collect::<BTreeSet<_>>(),
            inventory: items.into_iter().map(|(i, q)| (i, q as u32)).collect::<BTreeMap<_, _>>(),
            equipment: Equipment {
                main: r.get("equip_main"),
                off: r.get("equip_off"),
                armor: r.get("equip_armor"),
                spell1: r.get("equip_spell1"),
                spell2: r.get("equip_spell2"),
            },
            remnant,
            version: r.get("version"),
        }))
    }

    /// Writes a full character aggregate. Returns `false` without changing
    /// anything when a newer version is already stored, which makes
    /// out-of-order or replayed saves harmless.
    pub async fn save_character(&self, save: &SaveRequest) -> DbResult<bool> {
        let c = &save.data;
        let id = parse_uuid(&c.id)?;
        let mut tx = self.pool.begin().await?;
        let (rx, ry, ra) = match c.remnant {
            Some(r) => (Some(r.pos.x), Some(r.pos.y), Some(r.amount as i64)),
            None => (None, None, None),
        };
        let updated = sqlx::query(
            "update characters set
                vitality = $2, endurance = $3, might = $4, finesse = $5, occult = $6,
                echoes = $7, hp = $8, draughts = $9, draughts_max = $10, pos_x = $11, pos_y = $12,
                checkpoint = $13, equip_main = $14, equip_off = $15, equip_armor = $16,
                equip_spell1 = $17, equip_spell2 = $18, remnant_x = $19, remnant_y = $20,
                remnant_echoes = $21, version = $22, updated_at = now()
             where id = $1 and version < $22",
        )
        .bind(id)
        .bind(c.attrs[0] as i32)
        .bind(c.attrs[1] as i32)
        .bind(c.attrs[2] as i32)
        .bind(c.attrs[3] as i32)
        .bind(c.attrs[4] as i32)
        .bind(c.echoes as i64)
        .bind(c.hp as i32)
        .bind(c.draughts as i16)
        .bind(c.draughts_max as i16)
        .bind(c.pos.x)
        .bind(c.pos.y)
        .bind(&c.checkpoint)
        .bind(&c.equipment.main)
        .bind(&c.equipment.off)
        .bind(&c.equipment.armor)
        .bind(&c.equipment.spell1)
        .bind(&c.equipment.spell2)
        .bind(rx)
        .bind(ry)
        .bind(ra)
        .bind(c.version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query("delete from character_items where character_id = $1").bind(id).execute(&mut *tx).await?;
        sqlx::query("delete from character_flags where character_id = $1").bind(id).execute(&mut *tx).await?;
        sqlx::query("delete from character_lanterns where character_id = $1").bind(id).execute(&mut *tx).await?;
        write_children(&mut tx, id, c).await?;
        if !save.ledger.is_empty() {
            let kinds: Vec<&str> = save.ledger.iter().map(|l| l.kind).collect();
            let deltas: Vec<i64> = save.ledger.iter().map(|l| l.delta).collect();
            let balances: Vec<i64> = save.ledger.iter().map(|l| l.balance as i64).collect();
            let refs: Vec<&str> = save.ledger.iter().map(|l| l.reference.as_str()).collect();
            sqlx::query(
                "insert into echo_ledger (character_id, kind, delta, balance, reference, version)
                 select $1, k, d, b, r, $6 from unnest($2::text[], $3::bigint[], $4::bigint[], $5::text[]) as t(k, d, b, r)",
            )
            .bind(id)
            .bind(&kinds)
            .bind(&deltas)
            .bind(&balances)
            .bind(&refs)
            .bind(c.version)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    pub async fn ledger(&self, character: &str) -> DbResult<Vec<(String, i64, i64)>> {
        let id = parse_uuid(character)?;
        Ok(sqlx::query_as("select kind, delta, balance from echo_ledger where character_id = $1 order by id")
            .bind(id)
            .fetch_all(&self.pool)
            .await?)
    }

    // ---- moderation --------------------------------------------------

    pub async fn audit(
        &self,
        actor: Option<Uuid>,
        action: &str,
        target: &str,
        detail: serde_json::Value,
    ) -> DbResult<()> {
        sqlx::query("insert into audit_log (actor, action, target, detail) values ($1, $2, $3, $4)")
            .bind(actor)
            .bind(action)
            .bind(target)
            .bind(detail)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn suspend(
        &self,
        username: &str,
        until: Option<DateTime<Utc>>,
        reason: &str,
        actor: Option<Uuid>,
    ) -> DbResult<Uuid> {
        let id = self.account_id_by_name(username).await?.ok_or(DbError::NotFound)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("update accounts set suspended_until = $2, suspend_reason = $3 where id = $1")
            .bind(id)
            .bind(until)
            .bind(reason)
            .execute(&mut *tx)
            .await?;
        sqlx::query("update sessions set revoked_at = now() where account_id = $1 and revoked_at is null")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("insert into audit_log (actor, action, target, detail) values ($1, $2, $3, $4)")
            .bind(actor)
            .bind(if until.is_some() { "suspend" } else { "unsuspend" })
            .bind(username)
            .bind(serde_json::json!({ "until": until, "reason": reason }))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn set_admin(&self, username: &str, admin: bool) -> DbResult<()> {
        let n = sqlx::query("update accounts set is_admin = $2 where lower(username) = lower($1)")
            .bind(username)
            .bind(admin)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n == 0 {
            return Err(DbError::NotFound);
        }
        self.audit(None, if admin { "grant_admin" } else { "revoke_admin" }, username, serde_json::json!({})).await
    }

    pub async fn suspended_accounts(&self) -> DbResult<Vec<Uuid>> {
        Ok(sqlx::query_scalar("select id from accounts where suspended_until > now()").fetch_all(&self.pool).await?)
    }

    pub async fn set_mute(&self, account: Uuid, target: Uuid, on: bool) -> DbResult<()> {
        if on {
            sqlx::query("insert into account_mutes (account_id, muted_id) values ($1, $2) on conflict do nothing")
                .bind(account)
                .bind(target)
                .execute(&self.pool)
                .await?;
        } else {
            sqlx::query("delete from account_mutes where account_id = $1 and muted_id = $2")
                .bind(account)
                .bind(target)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn mutes(&self, account: Uuid) -> DbResult<Vec<Uuid>> {
        Ok(sqlx::query_scalar("select muted_id from account_mutes where account_id = $1")
            .bind(account)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn report(&self, reporter: Uuid, reported: Uuid, reason: &str) -> DbResult<()> {
        sqlx::query("insert into reports (reporter, reported, reason) values ($1, $2, $3)")
            .bind(reporter)
            .bind(reported)
            .bind(reason)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn open_reports(&self) -> DbResult<Vec<(i64, String, String, String, DateTime<Utc>)>> {
        Ok(sqlx::query_as(
            "select r.id, a.username, b.username, r.reason, r.created_at from reports r
             join accounts a on a.id = r.reporter join accounts b on b.id = r.reported
             where r.resolved_at is null order by r.id",
        )
        .fetch_all(&self.pool)
        .await?)
    }
}

async fn write_children(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: Uuid, c: &CharacterData) -> DbResult<()> {
    let items: Vec<&str> = c.inventory.keys().map(String::as_str).collect();
    let qtys: Vec<i32> = c.inventory.values().map(|q| *q as i32).collect();
    sqlx::query("insert into character_items (character_id, item, qty) select $1, i, q from unnest($2::text[], $3::int[]) as t(i, q)")
        .bind(id)
        .bind(&items)
        .bind(&qtys)
        .execute(&mut **tx)
        .await?;
    let flags: Vec<&str> = c.flags.iter().map(String::as_str).collect();
    sqlx::query("insert into character_flags (character_id, flag) select $1, f from unnest($2::text[]) as t(f)")
        .bind(id)
        .bind(&flags)
        .execute(&mut **tx)
        .await?;
    let lanterns: Vec<&str> = c.lanterns.iter().map(String::as_str).collect();
    sqlx::query("insert into character_lanterns (character_id, lantern) select $1, l from unnest($2::text[]) as t(l)")
        .bind(id)
        .bind(&lanterns)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn parse_uuid(s: &str) -> DbResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| DbError::Corrupt(format!("bad uuid {s}")))
}

fn parse_archetype(s: &str) -> DbResult<Archetype> {
    Archetype::from_id(s).ok_or_else(|| DbError::Corrupt(format!("unknown archetype {s}")))
}
