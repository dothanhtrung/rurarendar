create table if not exists events
(
    id         INTEGER           not null
        primary key,
    sync_id    INTEGER
        unique,
    sync_key   TEXT              not null
        unique,
    payload    TEXT              not null,
    rev        INTEGER           not null,
    deleted    INTEGER default 0 not null,
    profile_id INTEGER default 0 not null
);

create table if not exists outbox
(
    item_id   INTEGER           not null
        primary key,
    item_type TEXT              not null,
    rev       INTEGER           not null,
    base_rev  INTEGER           not null,
    deleted   INTEGER default 0 not null
);

create table if not exists profiles
(
    id      INTEGER            not null
        primary key autoincrement,
    name    TEXT               not null,
    enabled INTEGER default 1  not null,
    catalog TEXT    default '' not null,
    sort    INTEGER default 0  not null
);
