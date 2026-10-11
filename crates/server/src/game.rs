//! The world actor: owns the simulation, every in-world connection's
//! outbound queue, parties and chat routing. Nothing else touches `World`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hollowrite_game_core::world::{CommandError, Out, World};
use hollowrite_game_core::{CharacterData, Content};
use hollowrite_persistence::Db;
use hollowrite_protocol::{
    sanitize_text, Action, ChatChannel, ClientMsg, Dir8, EntityId, ErrorCode, GameEvent, PartyMember, ServerMsg,
    MAX_CHAT_CHARS,
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::metrics::Metrics;
use crate::persist::Persist;
use crate::ratelimit::Bucket;

pub const MAX_PARTY: usize = 4;
const LOCAL_CHAT_RADIUS: i32 = 24_000;
const OUTBOUND_CAPACITY: usize = 512;
const SLOW_CONSUMER_DROPS: u32 = 45;
const PARTY_REFRESH_TICKS: u64 = 15;

pub type ConnId = u64;

#[derive(Debug)]
pub enum Outbound {
    Frame(String),
    Close(String),
}

pub fn outbound_channel() -> (mpsc::Sender<Outbound>, mpsc::Receiver<Outbound>) {
    mpsc::channel(OUTBOUND_CAPACITY)
}

pub enum ToWorld {
    Join {
        conn: ConnId,
        account: Uuid,
        data: Box<CharacterData>,
        tx: mpsc::Sender<Outbound>,
        mutes: Vec<Uuid>,
        reply: oneshot::Sender<()>,
    },
    Input {
        conn: ConnId,
        seq: u32,
        mv: Option<Dir8>,
        act: Option<Action>,
        face: Option<Dir8>,
    },
    Msg {
        conn: ConnId,
        msg: ClientMsg,
    },
    Leave {
        conn: ConnId,
    },
    KickAccounts(Vec<Uuid>),
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

#[derive(Clone)]
pub struct WorldHandle {
    tx: mpsc::Sender<ToWorld>,
}

impl WorldHandle {
    pub async fn send(&self, msg: ToWorld) -> bool {
        self.tx.send(msg).await.is_ok()
    }

    /// Inputs are lossy under overload: a newer frame supersedes them anyway.
    pub fn try_send(&self, msg: ToWorld) -> bool {
        self.tx.try_send(msg).is_ok()
    }
}

struct Conn {
    account: Uuid,
    entity: EntityId,
    name: String,
    tx: mpsc::Sender<Outbound>,
    known: HashSet<EntityId>,
    chat: Bucket,
    mutes: HashSet<Uuid>,
    dropped: u32,
    events: Vec<GameEvent>,
    sheet_dirty: bool,
}

struct Party {
    leader: EntityId,
    members: Vec<EntityId>,
}

struct Actor {
    world: World,
    content: Arc<Content>,
    conns: HashMap<ConnId, Conn>,
    by_entity: HashMap<EntityId, ConnId>,
    parties: HashMap<u64, Party>,
    party_of: HashMap<EntityId, u64>,
    invites: HashMap<EntityId, BTreeSet<EntityId>>,
    next_party: u64,
    persist: Persist,
    db: Db,
    metrics: Arc<Metrics>,
}

pub fn spawn(
    content: Arc<Content>,
    seed: u64,
    persist: Persist,
    db: Db,
    metrics: Arc<Metrics>,
) -> (WorldHandle, tokio::task::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel(4096);
    let actor = Actor {
        world: World::new(content.clone(), seed),
        content,
        conns: HashMap::new(),
        by_entity: HashMap::new(),
        parties: HashMap::new(),
        party_of: HashMap::new(),
        invites: HashMap::new(),
        next_party: 1,
        persist,
        db,
        metrics,
    };
    let handle = tokio::spawn(actor.run(rx));
    (WorldHandle { tx }, handle)
}

fn encode(msg: &ServerMsg) -> String {
    serde_json::to_string(msg).expect("server messages always serialize")
}

impl Actor {
    async fn run(mut self, mut rx: mpsc::Receiver<ToWorld>) {
        let hz = self.content.balance.tick_hz;
        let budget = Duration::from_micros(1_000_000 / hz as u64);
        let mut interval = tokio::time::interval(budget);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                msg = rx.recv() => {
                    match msg {
                        Some(ToWorld::Shutdown { reply }) => {
                            self.shutdown();
                            let _ = reply.send(());
                            return;
                        }
                        Some(m) => self.handle(m),
                        None => {
                            self.shutdown();
                            return;
                        }
                    }
                }
                _ = interval.tick() => {
                    let start = Instant::now();
                    self.tick();
                    let us = start.elapsed().as_micros() as u64;
                    let m = &self.metrics;
                    m.tick_us_last.store(us, Ordering::Relaxed);
                    m.tick_us_max.fetch_max(us, Ordering::Relaxed);
                    Metrics::inc(&m.ticks);
                    if start.elapsed() > budget {
                        Metrics::inc(&m.tick_overruns);
                    }
                    m.world_heartbeat_ms.store(chrono::Utc::now().timestamp_millis() as u64, Ordering::Relaxed);
                }
            }
        }
    }

    fn shutdown(&mut self) {
        let ids: Vec<ConnId> = self.conns.keys().copied().collect();
        for id in ids {
            self.drop_conn(id, Some("The server is shutting down. Your progress has been saved."));
        }
        for save in self.world.save_all("shutdown") {
            self.persist.save(Box::new(save));
        }
    }

    fn send(&mut self, conn: ConnId, msg: &ServerMsg) {
        let Some(c) = self.conns.get(&conn) else { return };
        if c.tx.try_send(Outbound::Frame(encode(msg))).is_err() {
            Metrics::inc(&self.metrics.slow_consumer_kicks);
            tracing::warn!(conn, "reliable message could not be queued; dropping slow connection");
            self.drop_conn(conn, None);
        }
    }

    fn send_to_entity(&mut self, entity: EntityId, msg: &ServerMsg) {
        if let Some(&conn) = self.by_entity.get(&entity) {
            self.send(conn, msg);
        }
    }

    fn error(&mut self, conn: ConnId, code: ErrorCode, message: impl Into<String>) {
        self.send(conn, &ServerMsg::Error { code, message: message.into() });
    }

    fn system(&mut self, conn: ConnId, text: impl Into<String>) {
        self.send(conn, &ServerMsg::Chat { channel: ChatChannel::System, from: String::new(), text: text.into() });
    }

    fn handle(&mut self, msg: ToWorld) {
        match msg {
            ToWorld::Join { conn, account, data, tx, mutes, reply } => {
                self.join(conn, account, *data, tx, mutes);
                let _ = reply.send(());
            }
            ToWorld::Input { conn, seq, mv, act, face } => {
                if let Some(c) = self.conns.get(&conn) {
                    if !self.world.input(c.entity, seq, mv, act, face) {
                        Metrics::inc(&self.metrics.frames_rejected);
                    }
                }
            }
            ToWorld::Msg { conn, msg } => self.command(conn, msg),
            ToWorld::Leave { conn } => self.drop_conn(conn, None),
            ToWorld::KickAccounts(accounts) => {
                let hits: Vec<ConnId> =
                    self.conns.iter().filter(|(_, c)| accounts.contains(&c.account)).map(|(id, _)| *id).collect();
                for id in hits {
                    tracing::info!(conn = id, "disconnecting suspended account");
                    self.drop_conn(id, Some("This account has been suspended."));
                }
            }
            ToWorld::Shutdown { .. } => unreachable!("handled in run"),
        }
    }

    fn join(&mut self, conn: ConnId, account: Uuid, data: CharacterData, tx: mpsc::Sender<Outbound>, mutes: Vec<Uuid>) {
        // One controlling connection per character and per account: the
        // newest login wins and the older socket is told why it was closed.
        let existing = self.world.player_by_character(&data.id);
        let previous: Vec<ConnId> = self
            .conns
            .iter()
            .filter(|(_, c)| c.account == account || Some(c.entity) == existing)
            .map(|(id, _)| *id)
            .collect();
        for old in previous {
            let keep_entity = self.conns.get(&old).is_some_and(|c| Some(c.entity) == existing);
            if keep_entity {
                let c = self.conns.remove(&old).unwrap();
                self.by_entity.remove(&c.entity);
                let reason = "You logged in from another client.".to_string();
                let _ = c.tx.try_send(Outbound::Frame(encode(&ServerMsg::Kicked { reason: reason.clone() })));
                let _ = c.tx.try_send(Outbound::Close(reason));
            } else {
                self.drop_conn(old, Some("You logged in from another client."));
            }
        }
        let entity = match existing {
            Some(e) => e,
            None => self.world.join(data),
        };
        let name = self.world.players[&entity].data.name.clone();
        self.conns.insert(
            conn,
            Conn {
                account,
                entity,
                name: name.clone(),
                tx,
                known: HashSet::new(),
                chat: Bucket::new(5.0, 1.0),
                mutes: mutes.into_iter().collect(),
                dropped: 0,
                events: Vec::new(),
                sheet_dirty: false,
            },
        );
        self.by_entity.insert(entity, conn);
        self.metrics.players_online.store(self.world.players.len() as u64, Ordering::Relaxed);
        tracing::info!(conn, character = %name, entity, "entered world");
        self.send_enter(conn);
        let party = self.party_of.get(&entity).copied();
        self.world.set_party(entity, party);
        self.send_party(entity);
    }

    fn send_enter(&mut self, conn: ConnId) {
        let Some(c) = self.conns.get_mut(&conn) else { return };
        c.known.clear();
        let entity = c.entity;
        let Some(sheet) = self.world.sheet(entity) else { return };
        let msg =
            ServerMsg::EnterWorld { you: entity, tick: self.world.tick, map: self.content.map.to_protocol(), sheet };
        self.send(conn, &msg);
    }

    /// Removes a connection and its character from the world, saving it.
    fn drop_conn(&mut self, conn: ConnId, reason: Option<&str>) {
        let Some(c) = self.conns.remove(&conn) else { return };
        if let Some(r) = reason {
            let _ = c.tx.try_send(Outbound::Frame(encode(&ServerMsg::Kicked { reason: r.to_string() })));
            let _ = c.tx.try_send(Outbound::Close(r.to_string()));
        }
        self.by_entity.remove(&c.entity);
        self.leave_party(c.entity);
        self.invites.remove(&c.entity);
        for set in self.invites.values_mut() {
            set.remove(&c.entity);
        }
        if let Some(save) = self.world.leave(c.entity) {
            self.persist.save(Box::new(save));
        }
        self.metrics.players_online.store(self.world.players.len() as u64, Ordering::Relaxed);
        tracing::info!(conn, character = %c.name, "left world");
    }

    fn entity_by_name(&self, name: &str) -> Option<EntityId> {
        self.world.players.values().find(|p| p.data.name.eq_ignore_ascii_case(name)).map(|p| p.id)
    }

    fn command(&mut self, conn: ConnId, msg: ClientMsg) {
        let Some(c) = self.conns.get(&conn) else { return };
        let me = c.entity;
        let result = match msg {
            ClientMsg::Equip { slot, item } => self.world.equip(me, slot, item),
            ClientMsg::UseItem { item } => self.world.use_item(me, &item),
            ClientMsg::LevelUp { attr } => self.world.level_up(me, attr),
            ClientMsg::Talk { npc, choice } => self.world.talk(me, npc, choice),
            ClientMsg::Chat { channel, text } => {
                self.chat(conn, channel, &text);
                Ok(())
            }
            ClientMsg::PartyInvite { name } => {
                self.party_invite(conn, &name);
                Ok(())
            }
            ClientMsg::PartyAccept { from } => {
                self.party_accept(conn, &from);
                Ok(())
            }
            ClientMsg::PartyLeave => {
                self.leave_party(me);
                self.send_party(me);
                Ok(())
            }
            ClientMsg::Mute { name, on } => {
                self.mute(conn, &name, on);
                Ok(())
            }
            ClientMsg::Report { name, reason } => {
                self.report(conn, &name, &reason);
                Ok(())
            }
            ClientMsg::Ping { nonce } => {
                let tick = self.world.tick;
                self.send(conn, &ServerMsg::Pong { nonce, tick });
                Ok(())
            }
            ClientMsg::Resync => {
                self.send_enter(conn);
                self.send_party(me);
                Ok(())
            }
            ClientMsg::Leave => {
                self.drop_conn(conn, None);
                Ok(())
            }
            other => {
                tracing::debug!(conn, ?other, "unexpected message in world");
                Err(CommandError::NotAllowed("not available here"))
            }
        };
        match result {
            Ok(()) => {}
            Err(CommandError::NotFound) => self.error(conn, ErrorCode::NotFound, "nothing like that here"),
            Err(CommandError::NotAllowed(why)) => self.error(conn, ErrorCode::NotAllowed, why),
        }
    }

    fn chat(&mut self, conn: ConnId, channel: ChatChannel, text: &str) {
        let text = sanitize_text(text, MAX_CHAT_CHARS);
        if text.is_empty() {
            return;
        }
        let c = self.conns.get_mut(&conn).unwrap();
        if !c.chat.take() {
            self.error(conn, ErrorCode::RateLimited, "you are speaking too quickly");
            return;
        }
        let (me, account, name) = (c.entity, c.account, c.name.clone());
        let recipients: Vec<ConnId> = match channel {
            ChatChannel::Local => {
                let Some(pos) = self.world.players.get(&me).map(|p| p.body.pos) else { return };
                self.conns
                    .iter()
                    .filter(|(_, o)| {
                        self.world.players.get(&o.entity).is_some_and(|p| p.body.pos.within(pos, LOCAL_CHAT_RADIUS))
                    })
                    .map(|(id, _)| *id)
                    .collect()
            }
            ChatChannel::Party => {
                let Some(pid) = self.party_of.get(&me) else {
                    self.error(conn, ErrorCode::NotAllowed, "you are not in a party");
                    return;
                };
                self.parties[pid].members.iter().filter_map(|m| self.by_entity.get(m).copied()).collect()
            }
            ChatChannel::System => {
                self.error(conn, ErrorCode::NotAllowed, "that channel is read-only");
                return;
            }
        };
        tracing::info!(target: "chat", from = %name, ?channel, len = text.len(), "chat");
        let msg = ServerMsg::Chat { channel, from: name, text };
        for r in recipients {
            if self.conns.get(&r).is_some_and(|o| !o.mutes.contains(&account)) {
                self.send(r, &msg);
            }
        }
    }

    fn party_invite(&mut self, conn: ConnId, name: &str) {
        let me = self.conns[&conn].entity;
        let Some(target) = self.entity_by_name(name) else {
            self.error(conn, ErrorCode::NotFound, "no one by that name is here");
            return;
        };
        if target == me {
            self.error(conn, ErrorCode::NotAllowed, "you cannot invite yourself");
            return;
        }
        if let Some(pid) = self.party_of.get(&me) {
            if self.parties[pid].members.len() >= MAX_PARTY {
                self.error(conn, ErrorCode::NotAllowed, "your party is full");
                return;
            }
        }
        if self.party_of.contains_key(&me) && self.party_of.get(&me) == self.party_of.get(&target) {
            self.error(conn, ErrorCode::NotAllowed, "already in your party");
            return;
        }
        let invites = self.invites.entry(target).or_default();
        if invites.len() >= 8 && !invites.contains(&me) {
            self.error(conn, ErrorCode::RateLimited, "they have too many pending invitations");
            return;
        }
        invites.insert(me);
        let my_name = self.conns[&conn].name.clone();
        let target_name = self.world.players[&target].data.name.clone();
        self.system(conn, format!("You invite {target_name} to your party."));
        if let Some(&tc) = self.by_entity.get(&target) {
            self.system(tc, format!("{my_name} invites you to their party."));
        }
        self.send_party(target);
    }

    fn party_accept(&mut self, conn: ConnId, from: &str) {
        let me = self.conns[&conn].entity;
        let Some(inviter) = self.entity_by_name(from) else {
            self.error(conn, ErrorCode::NotFound, "that invitation has lapsed");
            return;
        };
        if !self.invites.get_mut(&me).is_some_and(|s| s.remove(&inviter)) {
            self.error(conn, ErrorCode::NotFound, "that invitation has lapsed");
            return;
        }
        let pid = match self.party_of.get(&inviter) {
            Some(p) => *p,
            None => {
                let pid = self.next_party;
                self.next_party += 1;
                self.parties.insert(pid, Party { leader: inviter, members: vec![inviter] });
                self.party_of.insert(inviter, pid);
                self.world.set_party(inviter, Some(pid));
                pid
            }
        };
        if self.parties[&pid].members.len() >= MAX_PARTY {
            self.error(conn, ErrorCode::NotAllowed, "that party is full");
            return;
        }
        self.leave_party(me);
        self.parties.get_mut(&pid).unwrap().members.push(me);
        self.party_of.insert(me, pid);
        self.world.set_party(me, Some(pid));
        let name = self.conns[&conn].name.clone();
        for m in self.parties[&pid].members.clone() {
            if let Some(&c) = self.by_entity.get(&m) {
                self.system(c, format!("{name} joins the party."));
            }
            self.send_party(m);
        }
    }

    fn leave_party(&mut self, entity: EntityId) {
        let Some(pid) = self.party_of.remove(&entity) else { return };
        self.world.set_party(entity, None);
        let party = self.parties.get_mut(&pid).unwrap();
        party.members.retain(|m| *m != entity);
        if party.leader == entity {
            if let Some(&next) = party.members.first() {
                party.leader = next;
            }
        }
        let remaining = party.members.clone();
        if remaining.len() <= 1 {
            self.parties.remove(&pid);
            for m in &remaining {
                self.party_of.remove(m);
                self.world.set_party(*m, None);
            }
        }
        let name = self.world.players.get(&entity).map(|p| p.data.name.clone()).unwrap_or_default();
        for m in remaining {
            if let Some(&c) = self.by_entity.get(&m) {
                self.system(c, format!("{name} leaves the party."));
            }
            self.send_party(m);
        }
    }

    fn send_party(&mut self, entity: EntityId) {
        let members = match self.party_of.get(&entity).and_then(|p| self.parties.get(p)) {
            Some(party) => party
                .members
                .iter()
                .filter_map(|m| self.world.players.get(m))
                .map(|p| PartyMember {
                    name: p.data.name.clone(),
                    online: true,
                    leader: self.parties[&self.party_of[&entity]].leader == p.id,
                    hp: Some([p.body.hp, p.body.hp_max]),
                })
                .collect(),
            None => vec![],
        };
        let invites = self
            .invites
            .get(&entity)
            .map(|s| s.iter().filter_map(|e| self.world.players.get(e)).map(|p| p.data.name.clone()).collect())
            .unwrap_or_default();
        self.send_to_entity(entity, &ServerMsg::Party { members, invites });
    }

    fn mute(&mut self, conn: ConnId, name: &str, on: bool) {
        let Some(target) = self.entity_by_name(name) else {
            self.error(conn, ErrorCode::NotFound, "no one by that name is here");
            return;
        };
        let Some(&tc) = self.by_entity.get(&target) else { return };
        let target_account = self.conns[&tc].account;
        let c = self.conns.get_mut(&conn).unwrap();
        if target_account == c.account {
            return;
        }
        if on {
            c.mutes.insert(target_account);
        } else {
            c.mutes.remove(&target_account);
        }
        let account = c.account;
        let db = self.db.clone();
        tokio::spawn(async move {
            if let Err(e) = db.set_mute(account, target_account, on).await {
                tracing::warn!(error = %e, "persisting mute failed");
            }
        });
        let shown = self.world.players[&target].data.name.clone();
        self.system(conn, if on { format!("You no longer hear {shown}.") } else { format!("You hear {shown} again.") });
    }

    fn report(&mut self, conn: ConnId, name: &str, reason: &str) {
        let reason = sanitize_text(reason, 500);
        let Some(target) = self.entity_by_name(name) else {
            self.error(conn, ErrorCode::NotFound, "no one by that name is here");
            return;
        };
        let Some(&tc) = self.by_entity.get(&target) else { return };
        let reported = self.conns[&tc].account;
        let reporter = self.conns[&conn].account;
        if reason.is_empty() || reported == reporter {
            self.error(conn, ErrorCode::BadRequest, "a report needs a reason");
            return;
        }
        let db = self.db.clone();
        tracing::warn!(target: "moderation", %reporter, %reported, "player report filed");
        tokio::spawn(async move {
            if let Err(e) = db.report(reporter, reported, &reason).await {
                tracing::warn!(error = %e, "storing report failed");
            }
        });
        self.system(conn, "Your report has been recorded for the moderators.");
    }

    fn route(&mut self, out: Out) {
        let r = self.content.balance.interest_radius;
        match out {
            Out::To(entity, e) => {
                if let Some(c) = self.by_entity.get(&entity).and_then(|id| self.conns.get_mut(id)) {
                    c.events.push(e);
                }
            }
            Out::Near(pos, e) => {
                for c in self.conns.values_mut() {
                    if self.world.players.get(&c.entity).is_some_and(|p| p.body.pos.chebyshev(pos) <= r) {
                        c.events.push(e.clone());
                    }
                }
            }
            Out::Dialogue(entity, d) => {
                let msg = ServerMsg::Dialogue { npc: d.npc, speaker: d.speaker, lines: d.lines, choices: d.choices };
                self.send_to_entity(entity, &msg);
            }
            Out::SheetChanged(entity) => {
                if let Some(c) = self.by_entity.get(&entity).and_then(|id| self.conns.get_mut(id)) {
                    c.sheet_dirty = true;
                }
            }
            Out::Save(req) => self.persist.save(req),
        }
    }

    fn tick(&mut self) {
        self.world.step();
        for out in self.world.drain() {
            self.route(out);
        }
        let tick = self.world.tick;
        let ids: Vec<ConnId> = self.conns.keys().copied().collect();
        for id in &ids {
            let Some(c) = self.conns.get_mut(id) else { continue };
            let events = std::mem::take(&mut c.events);
            let dirty = std::mem::replace(&mut c.sheet_dirty, false);
            let entity = c.entity;
            if !events.is_empty() {
                self.send(*id, &ServerMsg::Events { tick, events });
            }
            if dirty {
                if let Some(sheet) = self.world.sheet(entity) {
                    self.send(*id, &ServerMsg::Sheet { sheet });
                }
            }
        }
        // Commands handled above may have produced more output (saves, events
        // for the next frame); route them now so nothing waits a full tick.
        for out in self.world.drain() {
            self.route(out);
        }
        if tick.is_multiple_of(self.content.balance.snapshot_every as u64) {
            for id in &ids {
                self.send_snapshot(*id);
            }
        }
        if tick.is_multiple_of(PARTY_REFRESH_TICKS) {
            let members: Vec<EntityId> = self.party_of.keys().copied().collect();
            for m in members {
                self.send_party(m);
            }
        }
    }

    fn send_snapshot(&mut self, conn: ConnId) {
        let Some(c) = self.conns.get_mut(&conn) else { return };
        let Some(snap) = self.world.snapshot_for(c.entity, &mut c.known) else { return };
        let frame = encode(&ServerMsg::Snapshot(snap));
        let len = frame.len() as u64;
        match c.tx.try_send(Outbound::Frame(frame)) {
            Ok(()) => {
                c.dropped = 0;
                Metrics::inc(&self.metrics.snapshots_sent);
                self.metrics.snapshot_bytes.fetch_add(len, Ordering::Relaxed);
            }
            Err(_) => {
                // A dropped snapshot is superseded by the next one; the entity
                // set diff must restart so despawns are not lost.
                c.known.clear();
                c.dropped += 1;
                Metrics::inc(&self.metrics.snapshots_dropped);
                if c.dropped >= SLOW_CONSUMER_DROPS {
                    Metrics::inc(&self.metrics.slow_consumer_kicks);
                    tracing::warn!(conn, "client stopped reading; disconnecting");
                    self.drop_conn(conn, Some("Connection too slow."));
                }
            }
        }
    }
}
