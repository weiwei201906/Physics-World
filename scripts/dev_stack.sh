#!/usr/bin/env bash
# Start the whole Physics-World stack for local development.
#
# This machine has no usable container registry access, so instead of
# docker compose the stack runs from locally built binaries: PostgreSQL and
# Redis under $PW_DB_ROOT, five Rust services, an nginx gateway, and the
# prebuilt frontend bundle.
#
# Usage:
#   scripts/dev_stack.sh up|down|status
#
# Environment overrides:
#   PW_DEV_DIR   runtime directory for logs and pids (default /tmp/pwdev)
#   PW_ROOT      extracted nginx/redis/postgres packages (default /tmp/pwroot)
#   PW_HTTP_PORT gateway port (default 8080)
#   NGINX_BIN    nginx binary (default: system nginx, else $PW_ROOT/usr/sbin/nginx)
#   PW_SKIP_BUILD set to 1 to reuse existing cargo artifacts

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PW_DEV_DIR="${PW_DEV_DIR:-/tmp/pwdev}"
PW_ROOT="${PW_ROOT:-/tmp/pwroot}"
PW_HTTP_PORT="${PW_HTTP_PORT:-8080}"
QBANK_PORT="${QBANK_PORT:-15051}"
# Prefer an explicit path, then the nginx already installed on this host, and
# only fall back to the package extracted under PW_ROOT. The extracted copy is
# the last resort because installing packages system-wide needs root.
if [ -n "${NGINX_BIN:-}" ]; then
    :
elif command -v nginx >/dev/null 2>&1; then
    NGINX_BIN="$(command -v nginx)"
else
    NGINX_BIN="$PW_ROOT/usr/sbin/nginx"
fi
DB_SCRIPT="$ROOT/scripts/dev_db.sh"

DATABASE_URL="${DATABASE_URL:-postgres://physics_world:physics_world@127.0.0.1:15432/physics_world}"
REDIS_URL="${REDIS_URL:-redis://127.0.0.1:16379/0}"
STATIC_DIR="$ROOT/frontend/dist"
NGINX_DIR="$PW_DEV_DIR/nginx"
NGINX_CONF="$NGINX_DIR/nginx.conf"
NGINX_SERVER_CONF="$NGINX_DIR/default.conf"

clean_env=(
    env -u LD_LIBRARY_PATH -u LD_PRELOAD
    LC_ALL=C LANG=C
)

fail() { printf '%s\n' "$1" >&2; exit 1; }
note() { printf '%s\n' "$1"; }

port_open() {
    (exec 3<>"/dev/tcp/127.0.0.1/$1") >/dev/null 2>&1
}

wait_for_health() {
    local name="$1" url="$2"
    for _ in $(seq 1 50); do
        if curl -fsS -o /dev/null "$url" 2>/dev/null; then
            return 0
        fi
        sleep 0.2
    done
    note "$name did not become healthy; see $PW_DEV_DIR/$name.log"
    return 1
}

start_service() {
    local name="$1" port="$2" health_path="$3"
    shift 3
    local bin="$ROOT/backend/target/debug/$name"
    [ -x "$bin" ] || fail "missing binary: $bin (run scripts/dev_stack.sh up to build)"
    if port_open "$port"; then
        note "$name already listening on $port"
        return 0
    fi
    mkdir -p "$PW_DEV_DIR"
    nohup setsid "${clean_env[@]}" \
        DATABASE_URL="$DATABASE_URL" \
        REDIS_URL="$REDIS_URL" \
        RUST_LOG="${RUST_LOG:-info}" \
        "$@" \
        "$bin" >"$PW_DEV_DIR/$name.log" 2>&1 <"/dev/null" &
    echo $! >"$PW_DEV_DIR/$name.pid"
    wait_for_health "$name" "http://127.0.0.1:$port$health_path"
}

build_backend() {
    [ "${PW_SKIP_BUILD:-0}" = "1" ] && return 0
    note "building backend"
    (cd "$ROOT/backend" && "${clean_env[@]}" cargo build --offline 2>/dev/null \
        || "${clean_env[@]}" cargo build) || fail "cargo build failed"
}

build_frontend() {
    if [ -f "$STATIC_DIR/index.html" ]; then
        return 0
    fi
    note "building frontend"
    (cd "$ROOT/frontend" && "${clean_env[@]}" npm run build) || fail "frontend build failed"
}

# Question images live with the bundle; the gateway serves them as static
# assets so no object storage is required for local development.
sync_media() {
    local source="$ROOT/deploy/qbank/media"
    [ -d "$source" ] || return 0
    mkdir -p "$STATIC_DIR/media"
    cp "$source"/*.svg "$STATIC_DIR/media/"
}

# gRPC services expose no HTTP health endpoint; wait for the socket instead.
start_grpc_service() {
    local name="$1" port="$2"
    shift 2
    local bin="$ROOT/backend/target/debug/$name"
    [ -x "$bin" ] || fail "missing binary: $bin (run scripts/dev_stack.sh up to build)"
    if port_open "$port"; then
        note "$name already listening on $port"
        return 0
    fi
    mkdir -p "$PW_DEV_DIR"
    nohup setsid "${clean_env[@]}" \
        DATABASE_URL="$DATABASE_URL" \
        REDIS_URL="$REDIS_URL" \
        RUST_LOG="${RUST_LOG:-info}" \
        "$@" \
        "$bin" >"$PW_DEV_DIR/$name.log" 2>&1 <"/dev/null" &
    echo $! >"$PW_DEV_DIR/$name.pid"
    for _ in $(seq 1 50); do
        if port_open "$port"; then
            note "$name listening on $port"
            return 0
        fi
        sleep 0.2
    done
    fail "$name did not start; see $PW_DEV_DIR/$name.log"
}

render_gateway() {
    [ -x "$NGINX_BIN" ] || fail "nginx not found at $NGINX_BIN (install nginx, set NGINX_BIN=/path/to/nginx, or extract a package under $PW_ROOT)"
    mkdir -p "$NGINX_DIR/logs" "$NGINX_DIR/tmp/body" "$NGINX_DIR/tmp/proxy" \
        "$NGINX_DIR/tmp/fastcgi" "$NGINX_DIR/tmp/uwsgi" "$NGINX_DIR/tmp/scgi"
    export GATEWAY_PORT="$PW_HTTP_PORT"
    export AUTH_UPSTREAM="127.0.0.1:8081"
    export USER_API_UPSTREAM="127.0.0.1:8082"
    export SSE_UPSTREAM="127.0.0.1:8085"
    export REC_UPSTREAM="127.0.0.1:8086"
    export BATTLE_UPSTREAM="127.0.0.1:8087"
    export GATEWAY_STATIC_DIR="$STATIC_DIR"
    # Only the documented environment placeholders are substituted; nginx
    # runtime variables such as $uri and $http_authorization must survive.
    "${clean_env[@]}" envsubst \
        '${GATEWAY_PORT} ${AUTH_UPSTREAM} ${USER_API_UPSTREAM} ${SSE_UPSTREAM} ${REC_UPSTREAM} ${BATTLE_UPSTREAM} ${GATEWAY_STATIC_DIR}' \
        <"$ROOT/deploy/gateway/default.conf.template" >"$NGINX_SERVER_CONF" \
        || fail "gateway template rendering failed"
    cat >"$NGINX_CONF" <<EOF
worker_processes 1;
pid $NGINX_DIR/nginx.pid;
error_log $NGINX_DIR/logs/error.log warn;
events { worker_connections 1024; }
http {
    default_type application/octet-stream;
    access_log $NGINX_DIR/logs/access.log;
    client_body_temp_path $NGINX_DIR/tmp/body;
    proxy_temp_path $NGINX_DIR/tmp/proxy;
    fastcgi_temp_path $NGINX_DIR/tmp/fastcgi;
    uwsgi_temp_path $NGINX_DIR/tmp/uwsgi;
    scgi_temp_path $NGINX_DIR/tmp/scgi;
    # The extracted nginx package ships no mime.types, so declare the
    # extensions the frontend build emits explicitly.
    types {
        text/html html;
        text/css css;
        application/javascript js mjs;
        application/json json;
        image/svg+xml svg svgz;
        image/png png;
        image/jpeg jpg jpeg;
        image/gif gif;
        image/x-icon ico;
        font/woff2 woff2;
        font/woff woff;
        text/plain txt;
    }
    include $NGINX_SERVER_CONF;
}
EOF
    "${clean_env[@]}" "$NGINX_BIN" -t -c "$NGINX_CONF" -p "$NGINX_DIR" \
        || fail "nginx configuration is invalid"
}

start_gateway() {
    if port_open "$PW_HTTP_PORT"; then
        note "gateway already listening on $PW_HTTP_PORT"
        return 0
    fi
    "${clean_env[@]}" "$NGINX_BIN" -c "$NGINX_CONF" -p "$NGINX_DIR" \
        || fail "nginx failed to start; see $NGINX_DIR/logs"
    for _ in $(seq 1 50); do
        if curl -fsS -o /dev/null "http://127.0.0.1:$PW_HTTP_PORT/healthz" 2>/dev/null; then
            note "gateway listening on $PW_HTTP_PORT"
            return 0
        fi
        sleep 0.2
    done
    fail "gateway did not become healthy"
}

up() {
    [ -x "$DB_SCRIPT" ] || fail "missing $DB_SCRIPT"
    "${clean_env[@]}" bash "$DB_SCRIPT" start || fail "database startup failed"
    build_backend
    start_grpc_service question-bank "$QBANK_PORT" \
        QBANK_BIND_ADDR="127.0.0.1:$QBANK_PORT" \
        QBANK_BUNDLE="$ROOT/deploy/qbank/questions.json" \
        QBANK_MEDIA_BASE=/media
    start_service auth-service 8081 /health AUTH_BIND_ADDR=127.0.0.1:8081 AUTH_COOKIE_SECURE=false
    start_service user-api 8082 /health USER_API_BIND_ADDR=127.0.0.1:8082
    start_service user-api-sse 8085 /health PORT=8085 SSE_BIND_ADDR=127.0.0.1:8085
    start_service recommendation-service 8086 /health REC_PORT=8086 PORT=8086
    start_service battle-service 8087 /health BATTLE_BIND_ADDR=127.0.0.1:8087
    build_frontend
    sync_media
    render_gateway
    start_gateway
    note "site ready at http://127.0.0.1:$PW_HTTP_PORT"
}

stop_pid() {
    local name="$1" pid_file="$PW_DEV_DIR/$1.pid"
    [ -f "$pid_file" ] || return 0
    local pid
    pid="$(cat "$pid_file")"
    if kill "$pid" 2>/dev/null; then
        note "stopped $name"
    fi
    rm -f "$pid_file"
}

down() {
    if [ -f "$NGINX_DIR/nginx.pid" ]; then
        "${clean_env[@]}" "$NGINX_BIN" -c "$NGINX_CONF" -p "$NGINX_DIR" -s quit 2>/dev/null \
            && note "stopped gateway"
    fi
    for name in battle-service recommendation-service user-api-sse user-api auth-service \
                question-bank; do
        stop_pid "$name"
    done
    note "services stopped; databases are left running (scripts/dev_db.sh stop)"
}

status() {
    for entry in "question-bank $QBANK_PORT" "auth-service 8081" "user-api 8082" \
                 "user-api-sse 8085" "recommendation-service 8086" "battle-service 8087" \
                 "gateway $PW_HTTP_PORT"; do
        set -- $entry
        if port_open "$2"; then
            printf '%s: up (%s)\n' "$1" "$2"
        else
            printf '%s: down (%s)\n' "$1" "$2"
        fi
    done
}

case "${1:-}" in
    up) up ;;
    down) down ;;
    status) status ;;
    *)
        printf 'usage: %s up|down|status\n' "$0" >&2
        exit 2
        ;;
esac
