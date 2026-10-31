-- Initial schema for Physics-World.
-- Applies to PostgreSQL 18. The script is idempotent so it can be re-run by
-- the local dev helper without dropping data.

CREATE SCHEMA IF NOT EXISTS auth;
CREATE SCHEMA IF NOT EXISTS app;
CREATE SCHEMA IF NOT EXISTS question;
CREATE SCHEMA IF NOT EXISTS config;

CREATE TABLE IF NOT EXISTS auth.users (
    id uuid PRIMARY KEY,
    email text NOT NULL UNIQUE,
    password_hash text NOT NULL,
    nickname text NOT NULL,
    avatar_url text,
    roles text[] NOT NULL DEFAULT ARRAY['student']::text[],
    status text NOT NULL DEFAULT 'active',
    email_verified boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

CREATE TABLE IF NOT EXISTS auth.refresh_tokens (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    token_hash text NOT NULL UNIQUE,
    family_id uuid NOT NULL,
    expires_at timestamptz NOT NULL,
    rotated_at timestamptz,
    revoked boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS refresh_tokens_user_revoked_idx
    ON auth.refresh_tokens (user_id, revoked);
CREATE INDEX IF NOT EXISTS refresh_tokens_family_idx
    ON auth.refresh_tokens (family_id);

CREATE TABLE IF NOT EXISTS app.user_profiles (
    user_id uuid PRIMARY KEY,
    email text NOT NULL,
    nickname text NOT NULL,
    avatar_url text,
    roles text[] NOT NULL DEFAULT ARRAY['student']::text[],
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

CREATE TABLE IF NOT EXISTS app.quiz_sessions (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    mode text NOT NULL,
    chapter_id text,
    status text NOT NULL,
    current_index integer NOT NULL DEFAULT 0,
    total_questions integer NOT NULL,
    question_ids jsonb NOT NULL,
    started_at timestamptz NOT NULL,
    completed_at timestamptz
);

CREATE INDEX IF NOT EXISTS quiz_sessions_user_started_idx
    ON app.quiz_sessions (user_id, started_at DESC);

CREATE TABLE IF NOT EXISTS app.attempts (
    attempt_id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    session_id uuid NOT NULL,
    question_id text NOT NULL,
    is_correct boolean NOT NULL,
    answer text NOT NULL,
    submitted_at timestamptz NOT NULL,
    UNIQUE (session_id, question_id)
);

CREATE INDEX IF NOT EXISTS attempts_user_submitted_idx
    ON app.attempts (user_id, submitted_at DESC);

CREATE TABLE IF NOT EXISTS question.chapters (
    id text PRIMARY KEY,
    name text NOT NULL,
    position integer NOT NULL
);

CREATE TABLE IF NOT EXISTS question.questions (
    id text PRIMARY KEY,
    chapter_id text NOT NULL,
    difficulty integer NOT NULL,
    stem text NOT NULL,
    image_url text,
    explanation text,
    options jsonb NOT NULL,
    answer_option_id text NOT NULL,
    deleted_at timestamptz,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS questions_chapter_idx
    ON question.questions (chapter_id);

CREATE TABLE IF NOT EXISTS question.question_versions (
    id uuid PRIMARY KEY,
    question_id text NOT NULL,
    version integer NOT NULL,
    content jsonb NOT NULL,
    correct_option_id text NOT NULL,
    deleted_at timestamptz,
    created_at timestamptz NOT NULL,
    UNIQUE (question_id, version)
);

CREATE TABLE IF NOT EXISTS config.configs (
    namespace text NOT NULL,
    key text NOT NULL,
    value text NOT NULL,
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (namespace, key)
);

CREATE TABLE IF NOT EXISTS config.config_audit (
    id bigserial PRIMARY KEY,
    namespace text NOT NULL,
    key text NOT NULL,
    old_value text,
    new_value text,
    changed_by uuid,
    changed_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS config_audit_entry_idx
    ON config.config_audit (namespace, key, id DESC);

CREATE TABLE IF NOT EXISTS config.schemas (
    namespace text NOT NULL,
    key text NOT NULL,
    type_name text NOT NULL,
    required boolean NOT NULL,
    default_value text,
    description text,
    PRIMARY KEY (namespace, key)
);

INSERT INTO config.configs (namespace, key, value, updated_at) VALUES
    ('quiz', 'session.max_questions', '50', now()),
    ('quiz', 'submit.consistency_window_sec', '3', now()),
    ('leaderboard', 'page.size_max', '100', now()),
    ('battle', 'match.questions_per_room', '10', now()),
    ('recommendation', 'default_count', '10', now())
ON CONFLICT (namespace, key) DO NOTHING;

INSERT INTO config.schemas (namespace, key, type_name, required, default_value, description) VALUES
    ('quiz', 'session.max_questions', 'int', true, '50', 'Maximum questions per quiz session'),
    ('quiz', 'submit.consistency_window_sec', 'int', true, '3', 'Idempotency window for attempt submission'),
    ('leaderboard', 'page.size_max', 'int', true, '100', 'Maximum leaderboard page size')
ON CONFLICT (namespace, key) DO NOTHING;
