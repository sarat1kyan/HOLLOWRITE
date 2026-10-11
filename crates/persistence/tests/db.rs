//! Requires `TEST_DATABASE_URL`; skipped (with a note) when unset.

use std::sync::Arc;
use std::time::Duration;

use hollowrite_game_core::world::{LedgerEntry, SaveRequest};
use hollowrite_game_core::{CharacterData, Content, Pos};
use hollowrite_persistence::{Db, DbError, MAX_CHARACTERS_PER_ACCOUNT};
use hollowrite_protocol::{Appearance, Archetype};
use uuid::Uuid;

async fn db() -> Option<Db> {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL not set; skipping database test");
        return None;
    };
    let db = Db::connect(&url, 8).await.expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

fn content() -> Arc<Content> {
    Arc::new(Content::load(hollowrite_game_core::default_content_dir()).unwrap())
}

fn uniq(prefix: &str) -> String {
    format!("{prefix}{}", &Uuid::new_v4().simple().to_string()[..8])
}

async fn new_character(db: &Db, c: &Content, account: Uuid, name: &str) -> CharacterData {
    let data = CharacterData::new(
        c,
        Uuid::new_v4().to_string(),
        account.to_string(),
        name.into(),
        Archetype::AshWarden,
        Appearance { sigil: 1, hue: 2 },
    );
    db.insert_character(&data).await.unwrap();
    data
}

#[tokio::test]
async fn account_names_are_unique_case_insensitively() {
    let Some(db) = db().await else { return };
    let name = uniq("Acct");
    db.create_account(&name, "hash").await.unwrap();
    let err = db.create_account(&name.to_uppercase(), "hash").await.unwrap_err();
    assert!(matches!(err, DbError::Conflict));
    assert!(db.account_by_name(&name.to_lowercase()).await.unwrap().is_some());
}

#[tokio::test]
async fn sessions_resolve_until_revoked() {
    let Some(db) = db().await else { return };
    let acct = db.create_account(&uniq("Sess"), "hash").await.unwrap();
    let token = db.create_session(acct, "web", Duration::from_secs(3600)).await.unwrap();
    let s = db.session(&token).await.unwrap().expect("session valid");
    assert_eq!(s.account_id, acct);
    assert!(db.session("not-a-token").await.unwrap().is_none());
    assert!(db.revoke_session(&token).await.unwrap());
    assert!(db.session(&token).await.unwrap().is_none());
    let expired = db.create_session(acct, "web", Duration::from_secs(0)).await.unwrap();
    assert!(db.session(&expired).await.unwrap().is_none());
}

#[tokio::test]
async fn suspension_revokes_sessions_and_is_audited() {
    let Some(db) = db().await else { return };
    let name = uniq("Bad");
    let acct = db.create_account(&name, "hash").await.unwrap();
    let token = db.create_session(acct, "terminal", Duration::from_secs(3600)).await.unwrap();
    db.suspend(&name, Some(chrono::Utc::now() + chrono::Duration::hours(1)), "spam", None).await.unwrap();
    assert!(db.session(&token).await.unwrap().is_none());
    assert!(db.suspended_accounts().await.unwrap().contains(&acct));
    let fresh = db.create_session(acct, "terminal", Duration::from_secs(3600)).await.unwrap();
    assert!(db.session(&fresh).await.unwrap().is_none(), "suspended accounts cannot use new sessions");
    let n: i64 = sqlx::query_scalar("select count(*) from audit_log where target = $1 and action = 'suspend'")
        .bind(&name)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn character_round_trips_through_the_database() {
    let Some(db) = db().await else { return };
    let c = content();
    let acct = db.create_account(&uniq("Rt"), "hash").await.unwrap();
    let mut data = new_character(&db, &c, acct, &uniq("Hero")).await;
    data.echoes = 1234;
    data.attrs[2] += 3;
    data.pos = Pos::new(66_500, 28_500);
    data.checkpoint = "gloam_lantern".into();
    data.flags.insert("q:light_plaza".into());
    data.lanterns.insert("gloam_lantern".into());
    data.inventory.insert("ashen_moth".into(), 3);
    data.equipment.main = Some("penitent_maul".into());
    data.remnant = Some(hollowrite_game_core::character::Remnant { pos: Pos::new(1000, 2000), amount: 77 });
    data.version = 1;
    let ok = db
        .save_character(&SaveRequest {
            data: data.clone(),
            reason: "test",
            ledger: vec![LedgerEntry { kind: "kill", delta: 60, balance: 1234, reference: "x".into() }],
        })
        .await
        .unwrap();
    assert!(ok);
    let loaded = db.load_character(acct, &data.id).await.unwrap().unwrap();
    assert_eq!(loaded, data);
    let other = db.create_account(&uniq("Thief"), "hash").await.unwrap();
    assert!(db.load_character(other, &data.id).await.unwrap().is_none(), "characters are scoped to their account");
    assert_eq!(db.ledger(&data.id).await.unwrap(), vec![("kill".to_string(), 60, 1234)]);
}

#[tokio::test]
async fn stale_saves_are_rejected() {
    let Some(db) = db().await else { return };
    let c = content();
    let acct = db.create_account(&uniq("Ver"), "hash").await.unwrap();
    let mut data = new_character(&db, &c, acct, &uniq("Ver")).await;
    data.version = 5;
    data.echoes = 500;
    assert!(db.save_character(&SaveRequest { data: data.clone(), reason: "t", ledger: vec![] }).await.unwrap());
    let mut old = data.clone();
    old.version = 4;
    old.echoes = 999_999;
    assert!(!db.save_character(&SaveRequest { data: old, reason: "t", ledger: vec![] }).await.unwrap());
    let replay = data.clone();
    assert!(!db.save_character(&SaveRequest { data: replay, reason: "t", ledger: vec![] }).await.unwrap());
    assert_eq!(db.load_character(acct, &data.id).await.unwrap().unwrap().echoes, 500);
}

#[tokio::test]
async fn concurrent_saves_of_one_character_converge_on_the_newest() {
    let Some(db) = db().await else { return };
    let c = content();
    let acct = db.create_account(&uniq("Race"), "hash").await.unwrap();
    let base = new_character(&db, &c, acct, &uniq("Race")).await;
    let mut tasks = Vec::new();
    for v in 1..=20i64 {
        let db = db.clone();
        let mut d = base.clone();
        d.version = v;
        d.echoes = v as u64 * 10;
        tasks.push(tokio::spawn(async move {
            db.save_character(&SaveRequest { data: d, reason: "t", ledger: vec![] }).await.unwrap()
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let loaded = db.load_character(acct, &base.id).await.unwrap().unwrap();
    assert_eq!(loaded.version, 20);
    assert_eq!(loaded.echoes, 200);
}

#[tokio::test]
async fn character_limit_holds_under_concurrency() {
    let Some(db) = db().await else { return };
    let c = content();
    let acct = db.create_account(&uniq("Many"), "hash").await.unwrap();
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let db = db.clone();
        let c = c.clone();
        tasks.push(tokio::spawn(async move {
            let d = CharacterData::new(
                &c,
                Uuid::new_v4().to_string(),
                acct.to_string(),
                uniq("Alt"),
                Archetype::GraveRunner,
                Appearance::default(),
            );
            db.insert_character(&d).await
        }));
    }
    let mut ok = 0;
    for t in tasks {
        if t.await.unwrap().is_ok() {
            ok += 1;
        }
    }
    assert_eq!(ok, MAX_CHARACTERS_PER_ACCOUNT);
    assert_eq!(db.list_characters(&c, acct).await.unwrap().len() as i64, MAX_CHARACTERS_PER_ACCOUNT);
}

#[tokio::test]
async fn database_constraints_reject_impossible_state() {
    let Some(db) = db().await else { return };
    let c = content();
    let acct = db.create_account(&uniq("Neg"), "hash").await.unwrap();
    let data = new_character(&db, &c, acct, &uniq("Neg")).await;
    let r = sqlx::query("update characters set echoes = -1 where id = $1")
        .bind(Uuid::parse_str(&data.id).unwrap())
        .execute(db.pool())
        .await;
    assert!(r.is_err(), "negative echoes must violate a constraint");
    let r = sqlx::query("update characters set remnant_echoes = 5 where id = $1")
        .bind(Uuid::parse_str(&data.id).unwrap())
        .execute(db.pool())
        .await;
    assert!(r.is_err(), "a remnant without a position must be rejected");
    let r = db.create_account("bad name!", "hash").await;
    assert!(r.is_err());
}
