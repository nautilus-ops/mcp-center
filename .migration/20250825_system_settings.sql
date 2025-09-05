CREATE TABLE IF NOT EXISTS tb_system_settings
(
    setting_name  TEXT PRIMARY KEY,
    setting_value TEXT NOT NULL
);

COMMENT ON TABLE tb_system_settings IS 'MCP-Center system settings';

COMMENT ON COLUMN tb_system_settings.setting_name IS 'MCP-Center system setting name';
COMMENT ON COLUMN tb_system_settings.setting_value IS 'MCP-Center system setting value';

INSERT INTO tb_system_settings (setting_name, setting_value)
VALUES ('SELF_ADDRESS', 'http://127.0.0.1')
ON CONFLICT (setting_name)
    DO UPDATE SET setting_value = EXCLUDED.setting_value;