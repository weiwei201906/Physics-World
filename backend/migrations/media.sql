-- Per-question media variants and content scope.
--
-- Idempotent so the local dev helper can re-apply it. The filename has no
-- underscore prefix on purpose: sqlx::migrate! ignores files without a
-- numeric version prefix, matching init.sql and consumers.sql.

-- Media entries: [{"url": "...", "alt": "...", "variant": "default",
--                  "width": 640, "height": 480, "mime": "image/svg+xml"}]
-- The first entry is the primary image; image_url stays as the canonical
-- single-image field for older clients.
ALTER TABLE question.questions
    ADD COLUMN IF NOT EXISTS media jsonb NOT NULL DEFAULT '[]'::jsonb;

-- Scope selects which audience a question belongs to, for example
-- {"grade": "10", "subject": "physics", "tenant": "demo"}.
ALTER TABLE question.questions
    ADD COLUMN IF NOT EXISTS scope jsonb NOT NULL DEFAULT '{}'::jsonb;

CREATE INDEX IF NOT EXISTS questions_scope_gin_idx
    ON question.questions USING gin (scope);
