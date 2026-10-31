#!/usr/bin/env bash
# End-to-end smoke test for the gateway, auth-service and user-api.
#
# The script starts the real Rust processes, renders the nginx gateway
# template, and exercises the public HTTP surface with curl.
#
# Usage:
#   NGINX_BIN=/usr/sbin/nginx scripts/smoke_backend.sh
#
# Environment overrides:
#   NGINX_BIN           nginx binary (defaults to `nginx` on PATH)
#   GATEWAY_STATIC_DIR  built frontend (defaults to frontend/dist)
#   AUTH_BIN            auth-service binary (defaults to target/debug)
#   USER_API_BIN        user-api binary (defaults to target/debug)
#   GATEWAY_PORT        gateway port (default 18080)
#   AUTH_PORT           auth-service port (default 18081)
#   USER_API_PORT       user-api port (default 18082)
#   KEEP_LOGS           1 to keep the temporary run directory
#   SMOKE_SKIP_DB       1 to skip starting and truncating the local database

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="${RUN_DIR:-$(mktemp -d)}"
GATEWAY_PORT="${GATEWAY_PORT:-18080}"
AUTH_PORT="${AUTH_PORT:-18081}"
USER_API_PORT="${USER_API_PORT:-18082}"
GATEWAY_STATIC_DIR="${GATEWAY_STATIC_DIR:-$ROOT/frontend/dist}"
AUTH_BIN="${AUTH_BIN:-$ROOT/backend/target/debug/auth-service}"
USER_API_BIN="${USER_API_BIN:-$ROOT/backend/target/debug/user-api}"
NGINX_BIN="${NGINX_BIN:-$(command -v nginx || true)}"

DATABASE_URL="postgres://physics_world:physics_world@127.0.0.1:15432/physics_world"
REDIS_URL="redis://127.0.0.1:16379/0"
export DATABASE_URL REDIS_URL

PSQL="$(command -v psql || true)"
REDIS_CLI="$(command -v redis-cli || true)"
if [ -z "$REDIS_CLI" ] && [ -x /tmp/pwdb/root/usr/bin/redis-cli ]; then
    REDIS_CLI=/tmp/pwdb/root/usr/bin/redis-cli
fi

FAILURES=0
BODY="$RUN_DIR/body.json"

ok() { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; FAILURES=$((FAILURES + 1)); }

if [ -z "$NGINX_BIN" ] || [ ! -x "$NGINX_BIN" ]; then
    printf 'nginx binary not found; set NGINX_BIN\n' >&2
    exit 1
fi
if [ ! -f "$GATEWAY_STATIC_DIR/index.html" ]; then
    printf 'frontend build missing at %s; run npm run build first\n' "$GATEWAY_STATIC_DIR" >&2
    exit 1
fi
for tool in curl jq envsubst; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'missing required tool: %s\n' "$tool" >&2
        exit 1
    fi
done

if [ ! -x "$AUTH_BIN" ] || [ ! -x "$USER_API_BIN" ]; then
    printf 'building backend binaries\n'
    ( cd "$ROOT/backend" && cargo build -p auth-service -p user-api ) || exit 1
fi

PIDS=()
cleanup() {
    for pid in "${PIDS[@]:-}"; do
        kill "$pid" 2>/dev/null
    done
    wait 2>/dev/null
    if [ "${KEEP_LOGS:-0}" != "1" ]; then
        rm -rf "$RUN_DIR"
    else
        printf 'logs kept in %s\n' "$RUN_DIR"
    fi
}
trap cleanup EXIT

if [ "${SMOKE_SKIP_DB:-0}" != "1" ]; then
    printf '\npreparing local database\n'
    ( cd "$ROOT" && bash scripts/dev_db.sh start ) || exit 1
    if [ -z "$PSQL" ]; then
        printf 'psql client not found\n' >&2
        exit 1
    fi
    env -u LD_LIBRARY_PATH -u LD_PRELOAD LC_ALL=C LANG=C \
        "$PSQL" "$DATABASE_URL" -v ON_ERROR_STOP=1 -q -c \
        'TRUNCATE TABLE auth.users, auth.refresh_tokens, app.user_profiles, app.quiz_sessions, app.attempts, question.chapters, question.questions, question.question_versions, config.config_audit' \
        || exit 1
fi

http_code() {
    local method=$1 path=$2
    shift 2
    curl -sS -o "$BODY" -w '%{http_code}' -X "$method" "$@" "$path"
}

expect_status() {
    local label=$1 expected=$2
    shift 2
    local code
    code=$(http_code "$@")
    if [ "$code" = "$expected" ]; then
        ok "$label ($code)"
    else
        bad "$label (expected $expected, got $code)"
        head -c 400 "$BODY" 2>/dev/null
        printf '\n'
    fi
}

expect_json() {
    local label=$1 filter=$2
    local value
    value=$(jq -r "$filter // empty" "$BODY" 2>/dev/null)
    if [ -n "$value" ]; then
        ok "$label ($value)"
    else
        bad "$label (missing $filter)"
        head -c 400 "$BODY" 2>/dev/null
        printf '\n'
    fi
}

pg_count() {
    env -u LD_LIBRARY_PATH -u LD_PRELOAD LC_ALL=C LANG=C \
        "$PSQL" "$DATABASE_URL" -tAc "$1" 2>/dev/null
}

expect_min_rows() {
    local label=$1 table=$2 min=$3 count
    count=$(pg_count "SELECT count(*) FROM $table")
    if [ -n "$count" ] && [ "$count" -ge "$min" ]; then
        ok "$label ($count rows)"
    else
        bad "$label (expected at least $min, got ${count:-unknown})"
    fi
}

wait_http() {
    local url=$1 name=$2
    for _ in $(seq 1 120); do
        if curl -fsS -o /dev/null "$url" 2>/dev/null; then
            return 0
        fi
        sleep 0.25
    done
    printf '%s did not become healthy\n' "$name" >&2
    return 1
}

printf 'run dir: %s\n' "$RUN_DIR"
mkdir -p "$RUN_DIR/nginx"

AUTH_BIND_ADDR="127.0.0.1:$AUTH_PORT" AUTH_COOKIE_SECURE=false \
    "$AUTH_BIN" >"$RUN_DIR/auth.log" 2>&1 &
AUTH_PID=$!
PIDS+=("$AUTH_PID")
USER_API_BIND_ADDR="127.0.0.1:$USER_API_PORT" \
    "$USER_API_BIN" >"$RUN_DIR/user-api.log" 2>&1 &
USER_API_PID=$!
PIDS+=("$USER_API_PID")

if ! wait_http "http://127.0.0.1:$AUTH_PORT/health" auth-service; then
    cat "$RUN_DIR/auth.log"; exit 1
fi
if ! wait_http "http://127.0.0.1:$USER_API_PORT/health" user-api; then
    cat "$RUN_DIR/user-api.log"; exit 1
fi

SSE_UPSTREAM="${SSE_UPSTREAM:-127.0.0.1:${SSE_PORT:-8085}}"
REC_UPSTREAM="${REC_UPSTREAM:-127.0.0.1:${REC_PORT:-8086}}"
BATTLE_UPSTREAM="${BATTLE_UPSTREAM:-127.0.0.1:${BATTLE_PORT:-8087}}"

GATEWAY_PORT="$GATEWAY_PORT" AUTH_UPSTREAM="127.0.0.1:$AUTH_PORT" \
USER_API_UPSTREAM="127.0.0.1:$USER_API_PORT" GATEWAY_STATIC_DIR="$GATEWAY_STATIC_DIR" \
SSE_UPSTREAM="$SSE_UPSTREAM" REC_UPSTREAM="$REC_UPSTREAM" BATTLE_UPSTREAM="$BATTLE_UPSTREAM" \
    envsubst '${GATEWAY_PORT} ${AUTH_UPSTREAM} ${USER_API_UPSTREAM} ${GATEWAY_STATIC_DIR} ${SSE_UPSTREAM} ${REC_UPSTREAM} ${BATTLE_UPSTREAM}' \
    <"$ROOT/deploy/gateway/default.conf.template" >"$RUN_DIR/nginx/default.conf"
cat >"$RUN_DIR/nginx/nginx.conf" <<EOF
worker_processes 1;
pid $RUN_DIR/nginx/nginx.pid;
error_log $RUN_DIR/nginx/error.log warn;
events { worker_connections 256; }
http {
    default_type application/octet-stream;
    access_log $RUN_DIR/nginx/access.log;
    client_body_temp_path $RUN_DIR/nginx/body;
    proxy_temp_path $RUN_DIR/nginx/proxy;
    fastcgi_temp_path $RUN_DIR/nginx/fastcgi;
    uwsgi_temp_path $RUN_DIR/nginx/uwsgi;
    scgi_temp_path $RUN_DIR/nginx/scgi;
    types {
        text/html html;
        text/css css;
        application/javascript js;
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
    include $RUN_DIR/nginx/default.conf;
}
EOF

"$NGINX_BIN" -c "$RUN_DIR/nginx/nginx.conf" -g 'daemon off;' \
    >"$RUN_DIR/nginx/stdout.log" 2>&1 &
PIDS+=($!)

GW="http://127.0.0.1:$GATEWAY_PORT"
if ! wait_http "$GW/healthz" gateway; then
    cat "$RUN_DIR/nginx/error.log" 2>/dev/null
    exit 1
fi

printf '\nhealth\n'
expect_status "auth-service health direct" 200 "GET" "http://127.0.0.1:$AUTH_PORT/health"
expect_status "user-api health direct" 200 "GET" "http://127.0.0.1:$USER_API_PORT/health"
expect_status "gateway healthz" 200 "GET" "$GW/healthz"
expect_status "gateway jwks" 200 "GET" "$GW/.well-known/jwks.json"
expect_json "jwks exposes at least one key" '.keys | length'

printf '\nregistration and login\n'
EMAIL="smoke-$(date +%s)@example.com"
PASSWORD='S3cretPassw0rd!'
expect_status "register" 201 "POST" "$GW/auth/register" \
    -H 'Content-Type: application/json' \
    --data "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\",\"nickname\":\"Smoke\"}"
USER_ID=$(jq -r '.id // empty' "$BODY")
expect_status "register duplicate" 409 "POST" "$GW/auth/register" \
    -H 'Content-Type: application/json' \
    --data "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}"
expect_status "login" 200 "POST" "$GW/auth/login" \
    -c "$RUN_DIR/cookies.txt" \
    -H 'Content-Type: application/json' \
    --data "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}"
cp "$BODY" "$RUN_DIR/login.json"
TOKEN=$(jq -r '.access_token // empty' "$RUN_DIR/login.json")
REFRESH=$(jq -r '.refresh_token // empty' "$RUN_DIR/login.json")
LOGIN_USER=$(jq -r '.user.id // empty' "$RUN_DIR/login.json")
[ -n "$TOKEN" ] && ok "login returned access token" || bad "login returned no access token"
[ "$USER_ID" = "$LOGIN_USER" ] && ok "login user matches registration" \
    || bad "login user mismatch ($USER_ID != $LOGIN_USER)"

printf '\nidentity subrequest\n'
AUTHZ="Authorization: Bearer $TOKEN"
CODE=$(curl -sS -D "$RUN_DIR/verify.headers" -o /dev/null -w '%{http_code}' \
    -H "$AUTHZ" "$GW/auth/verify")
[ "$CODE" = "204" ] && ok "auth verify with bearer ($CODE)" \
    || bad "auth verify with bearer (expected 204, got $CODE)"
if grep -qi "^x-user-id: $USER_ID" "$RUN_DIR/verify.headers"; then
    ok "auth verify returns identity header"
else
    bad "auth verify missing x-user-id"
fi
expect_status "auth verify without bearer" 401 "GET" "$GW/auth/verify"

printf '\nanonymous graphql\n'
expect_status "chapters" 200 "POST" "$GW/graphql" \
    -H 'Content-Type: application/json' \
    --data '{"query":"query { chapters { id name questionCount } }"}'
expect_json "chapters are seeded" '.data.chapters | length'
expect_status "leaderboard" 200 "POST" "$GW/graphql" \
    -H 'Content-Type: application/json' \
    --data '{"query":"query { leaderboard(period: DAILY, page: 1, size: 10) { total } }"}'
expect_status "me without token" 200 "POST" "$GW/graphql" \
    -H 'Content-Type: application/json' \
    --data '{"query":"query { me { id } }"}'
if jq -e '.errors[0].message | contains("unauthenticated")' "$BODY" >/dev/null 2>&1; then
    ok "me without token is rejected"
else
    bad "me without token did not fail with unauthenticated"
fi
expect_status "spoofed identity header is stripped" 200 "POST" "$GW/graphql" \
    -H 'Content-Type: application/json' -H "X-User-Id: $USER_ID" \
    --data '{"query":"query { me { id } }"}'
if jq -e '.errors[0].message | contains("unauthenticated")' "$BODY" >/dev/null 2>&1; then
    ok "gateway strips client identity headers"
else
    bad "client identity header reached user-api"
fi

printf '\nauthenticated graphql\n'
expect_status "me with token" 200 "POST" "$GW/graphql" \
    -H 'Content-Type: application/json' -H "$AUTHZ" \
    --data '{"query":"query { me { id email nickname roles } }"}'
expect_json "me returns the logged in user" '.data.me.id'
[ "$(jq -r '.data.me.email // empty' "$BODY")" = "$EMAIL" ] \
    && ok "me email matches login" || bad "me email mismatch"

expect_status "start quiz session" 200 "POST" "$GW/graphql" \
    -H 'Content-Type: application/json' -H "$AUTHZ" \
    --data '{"query":"mutation { startQuizSession(mode: PRACTICE) { id totalQuestions status } }"}'
SESSION_ID=$(jq -r '.data.startQuizSession.id // empty' "$BODY")
[ -n "$SESSION_ID" ] && ok "session created ($SESSION_ID)" || bad "session was not created"

jq -n --arg sid "$SESSION_ID" \
    '{query: "query($id: UUID!, $index: Int!) { sessionQuestion(sessionId: $id, index: $index) { id chapterId content { stem options { id } } } }", variables: {id: $sid, index: 0}}' \
    >"$RUN_DIR/session-question.json"
expect_status "session question" 200 "POST" "$GW/graphql" \
    -H 'Content-Type: application/json' -H "$AUTHZ" \
    --data-binary "@$RUN_DIR/session-question.json"
QUESTION_ID=$(jq -r '.data.sessionQuestion.id // empty' "$BODY")
[ -n "$QUESTION_ID" ] && ok "session question loaded ($QUESTION_ID)" \
    || bad "session question not returned"

if [ -n "$QUESTION_ID" ]; then
    jq -n --arg sid "$SESSION_ID" --arg qid "$QUESTION_ID" \
        '{query: "mutation($sid: UUID!, $qid: ID!, $answer: String!) { submitAttempt(input: {sessionId: $sid, questionId: $qid, answer: $answer}) { accepted attemptId correct } }", variables: {sid: $sid, qid: $qid, answer: "Z"}}' \
        >"$RUN_DIR/submit-attempt.json"
    expect_status "submit attempt" 200 "POST" "$GW/graphql" \
        -H 'Content-Type: application/json' -H "$AUTHZ" \
        --data-binary "@$RUN_DIR/submit-attempt.json"
    if jq -e '.data.submitAttempt.accepted == true' "$BODY" >/dev/null 2>&1; then
        ok "attempt accepted"
    else
        bad "attempt was not accepted"
    fi
fi

printf '\ngateway policy\n'
expect_status "invalid bearer is rejected" 401 "POST" "$GW/graphql" \
    -H 'Content-Type: application/json' -H 'Authorization: Bearer not-a-jwt' \
    --data '{"query":"query { chapters { id } }"}'
expect_status "config snapshot" 200 "GET" "$GW/config" -H "$AUTHZ"
expect_json "config snapshot has entries" '.entries | length'
expect_status "graphiql" 200 "GET" "$GW/graphiql" -H "$AUTHZ"
expect_status "battle websocket unavailable" 503 "GET" "$GW/ws/battle"
expect_status "spa root" 200 "GET" "$GW/"
if grep -q 'id="app"' "$BODY"; then ok "spa root serves index.html"; \
    else bad "spa root did not serve index.html"; fi
expect_status "spa fallback" 200 "GET" "$GW/practice"
if grep -q 'id="app"' "$BODY"; then ok "spa fallback serves index.html"; \
    else bad "spa fallback did not serve index.html"; fi

printf '\nrefresh and logout\n'
expect_status "refresh via cookie" 200 "POST" "$GW/auth/refresh" \
    -b "$RUN_DIR/cookies.txt" -H 'Content-Type: application/json' --data '{}'
REFRESH2=$(jq -r '.refresh_token // empty' "$BODY")
[ -n "$REFRESH2" ] && ok "refresh rotated the token" || bad "refresh returned no token"
if [ -n "$REFRESH2" ]; then
    expect_status "logout" 204 "POST" "$GW/auth/logout" \
        -H 'Content-Type: application/json' \
        --data "{\"refresh_token\":\"$REFRESH2\"}"
    expect_status "refresh after logout" 401 "POST" "$GW/auth/refresh" \
        -H 'Content-Type: application/json' \
        --data "{\"refresh_token\":\"$REFRESH2\"}"
fi

printf '\ndatabase persistence\n'
expect_min_rows "auth.users persisted" auth.users 1
expect_min_rows "auth.refresh_tokens persisted" auth.refresh_tokens 1
expect_min_rows "app.quiz_sessions persisted" app.quiz_sessions 1
expect_min_rows "app.attempts persisted" app.attempts 1
expect_min_rows "config.configs seeded" config.configs 5
REDIS_PING=$(env -u LD_LIBRARY_PATH -u LD_PRELOAD "$REDIS_CLI" -p 16379 ping 2>/dev/null)
if [ "$REDIS_PING" = "PONG" ]; then
    ok "redis ping ($REDIS_PING)"
else
    bad "redis ping (expected PONG, got ${REDIS_PING:-unknown})"
fi

printf '\nrestart persistence\n'
kill "$AUTH_PID" "$USER_API_PID" 2>/dev/null
wait "$AUTH_PID" 2>/dev/null
wait "$USER_API_PID" 2>/dev/null

AUTH_BIND_ADDR="127.0.0.1:$AUTH_PORT" AUTH_COOKIE_SECURE=false \
    "$AUTH_BIN" >>"$RUN_DIR/auth.log" 2>&1 &
AUTH_PID=$!
PIDS+=("$AUTH_PID")
USER_API_BIND_ADDR="127.0.0.1:$USER_API_PORT" \
    "$USER_API_BIN" >>"$RUN_DIR/user-api.log" 2>&1 &
USER_API_PID=$!
PIDS+=("$USER_API_PID")

if ! wait_http "http://127.0.0.1:$AUTH_PORT/health" auth-service; then
    cat "$RUN_DIR/auth.log"; exit 1
fi
if ! wait_http "http://127.0.0.1:$USER_API_PORT/health" user-api; then
    cat "$RUN_DIR/user-api.log"; exit 1
fi

expect_status "login after restart" 200 "POST" "$GW/auth/login" \
    -H 'Content-Type: application/json' \
    --data "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}"
NEW_TOKEN=$(jq -r '.access_token // empty' "$BODY")
[ -n "$NEW_TOKEN" ] && ok "restart login returned access token" \
    || bad "restart login returned no access token"

if [ -n "$SESSION_ID" ] && [ -n "$NEW_TOKEN" ]; then
    jq -n --arg sid "$SESSION_ID" \
        '{query: "query($id: UUID!) { quizSession(id: $id) { status currentIndex } }", variables: {id: $sid}}' \
        >"$RUN_DIR/session-after-restart.json"
    expect_status "quiz session after restart" 200 "POST" "$GW/graphql" \
        -H 'Content-Type: application/json' -H "Authorization: Bearer $NEW_TOKEN" \
        --data-binary "@$RUN_DIR/session-after-restart.json"
    STATUS=$(jq -r '.data.quizSession.status // empty' "$BODY")
    INDEX=$(jq -r '.data.quizSession.currentIndex // empty' "$BODY")
    if [ "$STATUS" = "COMPLETED" ] || { [ -n "$INDEX" ] && [ "$INDEX" -ge 1 ]; }; then
        ok "session survived restart (status=$STATUS, currentIndex=$INDEX)"
    else
        bad "session did not survive restart (status=${STATUS:-none}, currentIndex=${INDEX:-none})"
    fi
else
    bad "restart check skipped without session id or token"
fi

printf '\n'
if [ "$FAILURES" -eq 0 ]; then
    printf 'smoke test passed\n'
    exit 0
fi
printf 'smoke test failed: %d check(s)\n' "$FAILURES"
exit 1
