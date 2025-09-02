CREATE TABLE IF NOT EXISTS tb_api_keys
(
    apikey     TEXT PRIMARY KEY   DEFAULT gen_random_uuid(),
    name       TEXT      NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP
);

-- table comment
COMMENT ON TABLE tb_api_keys IS 'API key management table with soft delete support';
-- column comments
COMMENT ON COLUMN tb_api_keys.apikey IS 'API key as primary key, UUID format ensures uniqueness and security';
COMMENT ON COLUMN tb_api_keys.name IS 'API key name for identification and description of key purpose';
COMMENT ON COLUMN tb_api_keys.created_at IS 'Creation timestamp when the API key was generated';
COMMENT ON COLUMN tb_api_keys.updated_at IS 'Last modification timestamp of API key information';
COMMENT ON COLUMN tb_api_keys.deleted_at IS 'Deletion timestamp for soft delete, NULL means active, value means deleted';

-- Create trigger for tb_api_keys table (reusing existing function)
DO
$$
    BEGIN
        IF NOT EXISTS (SELECT 1
                       FROM pg_trigger
                       WHERE tgname = 'set_updated_at_trigger'
                         AND tgrelid = 'tb_api_keys'::regclass) THEN
            CREATE TRIGGER set_updated_at_trigger
                BEFORE UPDATE
                ON tb_api_keys
                FOR EACH ROW
            EXECUTE FUNCTION update_updated_at_column();
        END IF;
    END
$$;

-- Add trigger comment
COMMENT ON TRIGGER set_updated_at_trigger ON tb_api_keys IS 'Trigger to automatically update updated_at field using shared function';
