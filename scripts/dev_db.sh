#!/usr/bin/env bash
# Local PostgreSQL 18 and Redis 8 for development.
#
# The host has client tools only and no root, so the script fetches the
# distribution packages, extracts them under PW_DB_ROOT, and runs the
# servers from there.
#
# Usage:
#   scripts/dev_db.sh start|stop|status|reset
#
# Environment overrides:
#   PW_DB_ROOT     install and data root (default /tmp/pwdb)
#   PW_PG_PORT     PostgreSQL port (default 15432)
#   PW_REDIS_PORT  Redis port (default 16379)
#   PW_SKIP_DOWNLOAD  set to 1 to reuse extracted binaries

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PW_DB_ROOT="${PW_DB_ROOT:-/tmp/pwdb}"
PW_PG_PORT="${PW_PG_PORT:-15432}"
PW_REDIS_PORT="${PW_REDIS_PORT:-16379}"

PG_BIN="$PW_DB_ROOT/root/usr/lib/postgresql/18/bin"
LIB_DIR="$PW_DB_ROOT/root/usr/lib/x86_64-linux-gnu"
REDIS_BIN="$PW_DB_ROOT/root/usr/bin/redis-server"
if [ -x "$PW_DB_ROOT/root/usr/bin/redis-cli" ]; then
    REDIS_CLI="$PW_DB_ROOT/root/usr/bin/redis-cli"
else
    REDIS_CLI="$(command -v redis-cli || true)"
fi
PSQL="$(command -v psql || true)"
PG_ISREADY="$(command -v pg_isready || true)"
PG_DATA="$PW_DB_ROOT/data"
PG_LOG="$PW_DB_ROOT/pg.log"
REDIS_DIR="$PW_DB_ROOT/redis"
REDIS_PID="$PW_DB_ROOT/redis.pid"
REDIS_LOG="$PW_DB_ROOT/redis.log"

DB_USER="physics_world"
DB_PASSWORD="physics_world"
DB_NAME="physics_world"
DATABASE_URL="postgres://$DB_USER:$DB_PASSWORD@127.0.0.1:$PW_PG_PORT/$DB_NAME"
REDIS_URL="redis://127.0.0.1:$PW_REDIS_PORT/0"

run_clean() {
    env -u LD_LIBRARY_PATH -u LD_PRELOAD LC_ALL=C LANG=C \
        LD_LIBRARY_PATH="$LIB_DIR" "$@"
}

fail() { printf '%s\n' "$1" >&2; exit 1; }

fix_redis_binary() {
    # The redis-server package ships a symlink to the multicall redis binary,
    # which lives in redis-tools. When redis-tools is already installed we
    # copy that binary into the extracted tree so the symlink resolves.
    if [ -L "$REDIS_BIN" ] && [ ! -e "$REDIS_BIN" ]; then
        local source_bin
        source_bin="$(command -v redis-check-rdb || true)"
        [ -n "$source_bin" ] || return 1
        rm -f "$REDIS_BIN"
        cp "$source_bin" "$REDIS_BIN" || return 1
    fi
    [ -x "$REDIS_BIN" ]
}

ensure_binaries() {
    if [ -x "$PG_BIN/postgres" ] && [ -n "$PSQL" ] && fix_redis_binary; then
        return 0
    fi
    [ "${PW_SKIP_DOWNLOAD:-0}" = "1" ] && fail "binaries missing under $PW_DB_ROOT"

    mkdir -p "$PW_DB_ROOT/debs" "$PW_DB_ROOT/root"
    printf 'fetching database packages\n'
    (
        cd "$PW_DB_ROOT/debs" || exit 1
        run_clean apt-get download postgresql-18 redis-server redis-tools liburing2
    ) || fail "package download failed"
    for deb in "$PW_DB_ROOT"/debs/*.deb; do
        run_clean dpkg-deb -x "$deb" "$PW_DB_ROOT/root" || fail "extract failed: $deb"
    done
    [ -x "$PG_BIN/postgres" ] || fail "postgres binary missing after extraction"
    fix_redis_binary || fail "redis binary missing after extraction"
    PSQL="$(command -v psql || true)"
    PG_ISREADY="$(command -v pg_isready || true)"
    [ -n "$PSQL" ] || fail "psql client not found; install postgresql-client-18"
}

pg_running() {
    [ -d "$PG_DATA" ] && run_clean "$PG_BIN/pg_ctl" -D "$PG_DATA" status >/dev/null 2>&1
}

redis_running() {
    run_clean "$REDIS_CLI" -p "$PW_REDIS_PORT" ping >/dev/null 2>&1
}

init_pg() {
    [ -d "$PG_DATA" ] && return 0
    mkdir -p "$PG_DATA"
    printf 'initializing postgres cluster\n'
    run_clean "$PG_BIN/initdb" -D "$PG_DATA" -U postgres --auth=trust -E UTF8 --locale=C \
        >"$PW_DB_ROOT/initdb.log" 2>&1 || fail "initdb failed; see $PW_DB_ROOT/initdb.log"
}

start_pg() {
    if pg_running; then
        return 0
    fi
    init_pg
    printf 'starting postgres on %s\n' "$PW_PG_PORT"
    run_clean "$PG_BIN/pg_ctl" -D "$PG_DATA" \
        -o "-p $PW_PG_PORT -k $PW_DB_ROOT -c listen_addresses=127.0.0.1 -c max_connections=100" \
        -l "$PG_LOG" start >/dev/null || fail "postgres failed to start; see $PG_LOG"
    for _ in $(seq 1 30); do
        run_clean "$PG_ISREADY" -h 127.0.0.1 -p "$PW_PG_PORT" >/dev/null 2>&1 && return 0
        sleep 0.2
    done
    fail "postgres did not become ready; see $PG_LOG"
}

start_redis() {
    if redis_running; then
        return 0
    fi
    mkdir -p "$REDIS_DIR"
    printf 'starting redis on %s\n' "$PW_REDIS_PORT"
    run_clean "$REDIS_BIN" --port "$PW_REDIS_PORT" --bind 127.0.0.1 \
        --dir "$REDIS_DIR" --daemonize yes --pidfile "$REDIS_PID" --logfile "$REDIS_LOG" \
        || fail "redis failed to start; see $REDIS_LOG"
    for _ in $(seq 1 30); do
        redis_running && return 0
        sleep 0.2
    done
    fail "redis did not become ready; see $REDIS_LOG"
}

ensure_database() {
    local count
    count=$(run_clean "$PSQL" -h 127.0.0.1 -p "$PW_PG_PORT" -U postgres -d postgres -tAc \
        "SELECT 1 FROM pg_roles WHERE rolname='$DB_USER'") || fail "cannot query postgres"
    if [ -z "$count" ]; then
        run_clean "$PSQL" -h 127.0.0.1 -p "$PW_PG_PORT" -U postgres -d postgres -v ON_ERROR_STOP=1 \
            -c "CREATE ROLE $DB_USER LOGIN PASSWORD '$DB_PASSWORD'" >/dev/null || fail "role creation failed"
    fi
    count=$(run_clean "$PSQL" -h 127.0.0.1 -p "$PW_PG_PORT" -U postgres -d postgres -tAc \
        "SELECT 1 FROM pg_database WHERE datname='$DB_NAME'") || fail "cannot query postgres"
    if [ -z "$count" ]; then
        run_clean "$PSQL" -h 127.0.0.1 -p "$PW_PG_PORT" -U postgres -d postgres -v ON_ERROR_STOP=1 \
            -c "CREATE DATABASE $DB_NAME OWNER $DB_USER" >/dev/null || fail "database creation failed"
    fi
}

apply_migrations() {
    local file
    # Migrations are independent and idempotent; apply them in filename order.
    for file in $(ls -1 "$ROOT"/backend/migrations/*.sql 2>/dev/null | sort); do
        printf 'applying %s\n' "$(basename "$file")"
        run_clean "$PSQL" "postgres://$DB_USER:$DB_PASSWORD@127.0.0.1:$PW_PG_PORT/$DB_NAME" \
            -v ON_ERROR_STOP=1 -q -f "$file" || fail "migration failed: $file"
    done
}

print_env() {
    printf 'DATABASE_URL=%s\n' "$DATABASE_URL"
    printf 'REDIS_URL=%s\n' "$REDIS_URL"
}

start() {
    ensure_binaries
    start_pg
    start_redis
    ensure_database
    apply_migrations
    print_env
}

stop() {
    if pg_running; then
        printf 'stopping postgres\n'
        run_clean "$PG_BIN/pg_ctl" -D "$PG_DATA" stop -m fast >/dev/null || true
    fi
    if redis_running; then
        printf 'stopping redis\n'
        run_clean "$REDIS_CLI" -p "$PW_REDIS_PORT" shutdown nosave >/dev/null 2>&1 || true
    fi
}

status() {
    if pg_running; then
        printf 'postgres: running on %s\n' "$PW_PG_PORT"
    else
        printf 'postgres: stopped\n'
    fi
    if redis_running; then
        printf 'redis: running on %s\n' "$PW_REDIS_PORT"
    else
        printf 'redis: stopped\n'
    fi
    printf '%s\n' "$DATABASE_URL"
    printf '%s\n' "$REDIS_URL"
}

reset() {
    stop
    printf 'removing %s\n' "$PG_DATA"
    rm -rf "$PG_DATA" "$REDIS_DIR" "$PG_LOG" "$REDIS_LOG" "$REDIS_PID"
    start
}

case "${1:-}" in
    start) start ;;
    stop) stop ;;
    status) status ;;
    reset) reset ;;
    *)
        printf 'usage: %s start|stop|status|reset\n' "$0" >&2
        exit 2
        ;;
esac
