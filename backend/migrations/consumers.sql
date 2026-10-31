-- Battle persistence consumed by battle-result-consumer.
-- Applies to PostgreSQL 18. The script is idempotent so the local dev helper
-- can re-run it without dropping data.

CREATE SCHEMA IF NOT EXISTS battle;

CREATE TABLE IF NOT EXISTS battle.battles (
    battle_id uuid PRIMARY KEY,
    mode text NOT NULL,
    question_count integer NOT NULL,
    started_at timestamptz NOT NULL,
    finished_at timestamptz NOT NULL,
    winner_id uuid,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS battle.battle_participants (
    battle_id uuid NOT NULL,
    user_id uuid NOT NULL,
    score bigint NOT NULL,
    correct_count integer NOT NULL,
    rank integer NOT NULL,
    PRIMARY KEY (battle_id, user_id)
);

CREATE INDEX IF NOT EXISTS battle_participants_user_idx
    ON battle.battle_participants (user_id);
