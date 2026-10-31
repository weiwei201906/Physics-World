-- Per-user error vectors for adaptive practice and battle matchmaking.
-- A dimension is "chapter_id:difficulty" and its weight is the normalized
-- wrong-answer rate. The script is idempotent so dev_db.sh can re-run it.

CREATE TABLE IF NOT EXISTS app.user_vectors (
    user_id uuid PRIMARY KEY,
    dims text[] NOT NULL DEFAULT ARRAY[]::text[],
    weights double precision[] NOT NULL DEFAULT ARRAY[]::double precision[],
    attempts integer NOT NULL DEFAULT 0,
    wrong integer NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION app.refresh_user_vector(p_user uuid)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    v_dims text[];
    v_weights double precision[];
    v_attempts integer;
    v_wrong integer;
    v_norm double precision;
BEGIN
    WITH stats AS (
        SELECT q.chapter_id || ':' || q.difficulty AS dim,
               count(*) AS total,
               count(*) FILTER (WHERE NOT a.is_correct) AS wrong
        FROM app.attempts a
        JOIN question.questions q ON q.id = a.question_id
        WHERE a.user_id = p_user
        GROUP BY 1
    )
    SELECT coalesce(array_agg(dim ORDER BY dim), ARRAY[]::text[]),
           coalesce(array_agg(wrong::double precision / total ORDER BY dim),
                    ARRAY[]::double precision[])
    INTO v_dims, v_weights
    FROM stats;

    SELECT count(*), count(*) FILTER (WHERE NOT is_correct)
    INTO v_attempts, v_wrong
    FROM app.attempts
    WHERE user_id = p_user;

    SELECT sqrt(coalesce(sum(weight * weight), 0))
    INTO v_norm
    FROM unnest(v_weights) AS t(weight);

    IF v_norm > 0 THEN
        v_weights := ARRAY(
            SELECT weight / v_norm FROM unnest(v_weights) AS t(weight)
        );
    END IF;

    INSERT INTO app.user_vectors (user_id, dims, weights, attempts, wrong, updated_at)
    VALUES (p_user, v_dims, v_weights, v_attempts, v_wrong, now())
    ON CONFLICT (user_id) DO UPDATE SET
        dims = EXCLUDED.dims,
        weights = EXCLUDED.weights,
        attempts = EXCLUDED.attempts,
        wrong = EXCLUDED.wrong,
        updated_at = EXCLUDED.updated_at;
END;
$$;
