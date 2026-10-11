-- HOLLOWRITE initial schema.
-- Character state is stored as one aggregate (row + child tables) and is
-- always rewritten in a single transaction guarded by `version`.

create table accounts (
    id              uuid primary key,
    username        text not null,
    password_hash   text not null,
    is_admin        boolean not null default false,
    failed_logins   integer not null default 0 check (failed_logins >= 0),
    locked_until    timestamptz,
    suspended_until timestamptz,
    suspend_reason  text,
    created_at      timestamptz not null default now(),
    constraint accounts_username_format check (username ~ '^[A-Za-z][A-Za-z0-9_-]{2,15}$')
);
create unique index accounts_username_unique on accounts (lower(username));

create table sessions (
    id           uuid primary key,
    account_id   uuid not null references accounts (id) on delete cascade,
    token_hash   bytea not null unique,
    client       text not null check (client in ('web', 'terminal', 'bot')),
    created_at   timestamptz not null default now(),
    last_seen_at timestamptz not null default now(),
    expires_at   timestamptz not null,
    revoked_at   timestamptz
);
create index sessions_account on sessions (account_id);

create table characters (
    id             uuid primary key,
    account_id     uuid not null references accounts (id) on delete cascade,
    name           text not null,
    archetype      text not null check (archetype in ('ash_warden', 'veil_seer', 'grave_runner')),
    sigil          smallint not null check (sigil between 0 and 3),
    hue            smallint not null check (hue between 0 and 5),
    vitality       integer not null check (vitality between 1 and 99),
    endurance      integer not null check (endurance between 1 and 99),
    might          integer not null check (might between 1 and 99),
    finesse        integer not null check (finesse between 1 and 99),
    occult         integer not null check (occult between 1 and 99),
    echoes         bigint not null default 0 check (echoes >= 0),
    hp             integer not null check (hp >= 0),
    draughts       smallint not null check (draughts >= 0),
    draughts_max   smallint not null check (draughts_max between 0 and 20),
    pos_x          integer not null,
    pos_y          integer not null,
    checkpoint     text not null,
    equip_main     text,
    equip_off      text,
    equip_armor    text,
    equip_spell1   text,
    equip_spell2   text,
    remnant_x      integer,
    remnant_y      integer,
    remnant_echoes bigint check (remnant_echoes is null or remnant_echoes > 0),
    version        bigint not null default 0,
    created_at     timestamptz not null default now(),
    updated_at     timestamptz not null default now(),
    constraint characters_name_format check (name ~ '^[A-Za-z][A-Za-z0-9_-]{2,15}$'),
    constraint characters_remnant_complete check (
        (remnant_x is null) = (remnant_echoes is null) and (remnant_y is null) = (remnant_echoes is null)
    )
);
create unique index characters_name_unique on characters (lower(name));
create index characters_account on characters (account_id);

create table character_items (
    character_id uuid not null references characters (id) on delete cascade,
    item         text not null,
    qty          integer not null check (qty > 0),
    primary key (character_id, item)
);

create table character_flags (
    character_id uuid not null references characters (id) on delete cascade,
    flag         text not null,
    primary key (character_id, flag)
);

create table character_lanterns (
    character_id uuid not null references characters (id) on delete cascade,
    lantern      text not null,
    primary key (character_id, lantern)
);

-- Append-only economic audit trail written in the same transaction as the
-- aggregate it describes.
create table echo_ledger (
    id           bigserial primary key,
    character_id uuid not null references characters (id) on delete cascade,
    kind         text not null,
    delta        bigint not null,
    balance      bigint not null check (balance >= 0),
    reference    text not null default '',
    version      bigint not null,
    created_at   timestamptz not null default now()
);
create index echo_ledger_character on echo_ledger (character_id, id);

create table audit_log (
    id         bigserial primary key,
    actor      uuid references accounts (id) on delete set null,
    action     text not null,
    target     text,
    detail     jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now()
);

create table account_mutes (
    account_id uuid not null references accounts (id) on delete cascade,
    muted_id   uuid not null references accounts (id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (account_id, muted_id),
    check (account_id <> muted_id)
);

create table reports (
    id          bigserial primary key,
    reporter    uuid not null references accounts (id) on delete cascade,
    reported    uuid not null references accounts (id) on delete cascade,
    reason      text not null check (char_length(reason) between 1 and 500),
    created_at  timestamptz not null default now(),
    resolved_at timestamptz
);
