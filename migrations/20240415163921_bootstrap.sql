BEGIN;
CREATE OR REPLACE FUNCTION trigger_set_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;

$$ language 'plpgsql';
COMMIT;

CREATE EXTENSION IF NOT EXISTS pg_uuidv7;

CREATE TABLE cashbacks 
(
    id UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    currency_id TEXT NOT NULL,
    name_str TEXT NOT NULL,
    name_id TEXT NOT NULL,
    txid TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER SET_UPDATED_TIMESTAMP 
	BEFORE
	UPDATE
	    ON cashbacks FOR EACH ROW
	EXECUTE
	    PROCEDURE trigger_set_timestamp();
