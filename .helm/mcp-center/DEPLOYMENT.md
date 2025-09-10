# MCP Center Helm Chart Deployment Guide

## Deployment Options

### Option 1: Using Built-in PostgreSQL (Recommended for Development/Testing)

```bash
# Deploy with default configuration (includes PostgreSQL)
helm install mcp-center . --create-namespace --namespace mcp-center
```

### Option 2: Using External PostgreSQL (Recommended for Production)

1. Copy the example configuration file:
```bash
cp values-external-postgres.yaml my-values.yaml
```

2. Edit the configuration file and set your PostgreSQL connection information:
```yaml
postgresql:
  enabled: false

externalPostgresql:
  host: "your-postgres-host.example.com"
  port: 5432
  username: "mcpcenter"
  password: "your-secure-password"
  database: "mcpcenter"
```

3. Deploy with custom configuration:
```bash
helm install mcp-center . -f my-values.yaml --create-namespace --namespace mcp-center
```

## Configuration

### PostgreSQL Configuration

- `postgresql.enabled`: Whether to enable built-in PostgreSQL
  - `true`: Use built-in PostgreSQL (requires storage class support)
  - `false`: Use external PostgreSQL

### External PostgreSQL Configuration

When `postgresql.enabled: false`, configure the following parameters:

- `externalPostgresql.host`: PostgreSQL server address
- `externalPostgresql.port`: PostgreSQL port (default 5432)
- `externalPostgresql.username`: Database username
- `externalPostgresql.password`: Database password (stored securely in Kubernetes Secret)
- `externalPostgresql.database`: Database name

### MCP Admin Token Configuration

- `mcpAdminToken`: 32-character random string for MCP admin authentication
  - Default: Auto-generated 32-character random string
  - Custom: You can set your own token in values.yaml

## Environment Variables

The application will automatically receive the following environment variables:

- `POSTGRES_HOST`: PostgreSQL host address
- `POSTGRES_PORT`: PostgreSQL port
- `POSTGRES_USERNAME`: Database username
- `POSTGRES_PASSWORD`: Database password
- `POSTGRES_DATABASE`: Database name
- `MCP_ADMIN_TOKEN`: 32-character random string for admin authentication

## Troubleshooting

### Built-in PostgreSQL Cannot Start
- Check if the cluster has available storage classes
- Check if there is sufficient storage space
- View Pod logs: `kubectl logs -n mcp-center <pod-name>`

### External PostgreSQL Connection Failed
- Check network connectivity
- Verify if PostgreSQL service is accessible
- Check if username, password, and database name are correct