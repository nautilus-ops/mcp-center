CREATE TABLE IF NOT EXISTS tb_mcp_servers
(
    id             UUID PRIMARY KEY,
    name           TEXT      NOT NULL,
    tag            TEXT      NOT NULL,
    endpoint       TEXT      NOT NULL,
    transport_type TEXT      NOT NULL,
    create_from    TEXT      NOT NULL DEFAULT 'register',
    description    TEXT      NOT NULL DEFAULT '',
    extra          JSONB     NOT NULL DEFAULT '{}',
    created_at     TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at     TIMESTAMP
);

-- Unique index to avoid duplicate name + tag
CREATE UNIQUE INDEX IF NOT EXISTS uq_mcp_servers_name_tag_not_deleted
    ON tb_mcp_servers (name, tag)
    WHERE deleted_at IS NULL;

-- Table comment
COMMENT ON TABLE tb_mcp_servers IS 'MCP Server registry table';

-- Column comments
COMMENT ON COLUMN tb_mcp_servers.id IS 'Unique identifier (UUID)';
COMMENT ON COLUMN tb_mcp_servers.name IS 'MCP Server name';
COMMENT ON COLUMN tb_mcp_servers.tag IS 'MCP Server tag, used to distinguish different versions or instances';
COMMENT ON COLUMN tb_mcp_servers.endpoint IS 'MCP Server public endpoint (URL)';
COMMENT ON COLUMN tb_mcp_servers.transport_type IS 'Transport type, e.g. sse or streamable';
COMMENT ON COLUMN tb_mcp_servers.create_from IS 'MCP Server create from, e.g. manual, register, kubernetes-service';
COMMENT ON COLUMN tb_mcp_servers.extra IS 'Additional configuration information';
COMMENT ON COLUMN tb_mcp_servers.created_at IS 'Record creation time';
COMMENT ON COLUMN tb_mcp_servers.updated_at IS 'Last update time';
COMMENT ON COLUMN tb_mcp_servers.deleted_at IS 'Logical deletion time (NULL means not deleted)';

CREATE OR REPLACE FUNCTION update_updated_at_column()
    RETURNS TRIGGER AS
$$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DO
$$
    BEGIN
        IF NOT EXISTS (SELECT 1
                       FROM pg_trigger
                       WHERE tgname = 'set_updated_at_trigger'
                         AND tgrelid = 'tb_mcp_servers'::regclass) THEN
            CREATE TRIGGER set_updated_at_trigger
                BEFORE UPDATE
                ON tb_mcp_servers
                FOR EACH ROW
            EXECUTE FUNCTION update_updated_at_column();
        END IF;
    END
$$;