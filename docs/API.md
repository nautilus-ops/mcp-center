# MCP Center Easy Use

MCP Center is a centralized platform for managing and connecting MCP (Model Context Protocol) servers. This document describes the REST API interfaces provided by MCP Center.

## Basic Information

- **Base URL**: `http://localhost:5432`
- **Authentication**: Token
- **Content Type**: `application/json`

## Authentication

MCP Center uses Token for authentication. There are two types of authentication:

1. **Admin Token**: Admin token set via environment variable `MCP_ADMIN_TOKEN`
2. **API Keys**: API keys managed through the database

### Request Header Format

```http
Authorization: <your-token>
```

## API Endpoints

### 2. MCP Server Registry

#### Get All MCP Servers

```http
GET /api/registry/mcp-server
```

**Query Parameters**:
- `use_raw_endpoint` (optional): Whether to use raw endpoint, defaults to false
- `page_size` (optional): Page size, used together with page_num
- `page_num` (optional): Page number, used together with page_size

**Response**:
```json
{
  "servers": [
    {
      "id": "uuid",
      "name": "example-server",
      "tag": "1.0.0",
      "endpoint": "http://localhost:5432/proxy/connect/example-server/1.0.0",
      "transport_type": "sse",
      "description": "Example MCP server",
      "create_from": "register",
      "extra": null,
      "disabled": false,
      "created_at": "2024-01-01T00:00:00",
      "updated_at": "2024-01-01T00:00:00",
      "deleted_at": null
    }
  ],
  "count": 1
}
```

#### Register MCP Server

```http
POST /api/registry/mcp-server
```

**Request Body**:
```json
{
  "name": "example-server",
  "tag": "1.0.0",
  "endpoint": "http://127.0.0.1:8080/sse",
  "transport_type": "sse",
  "description": "Example MCP server",
  "extra": {
    "custom_field": "value"
  }
}
```

**Field Descriptions**:
- `name`: MCP server name (required)
- `tag`: Version tag (required)
- `endpoint`: Server endpoint URL (required)
- `transport_type`: Transport type, supports "sse" or "streamable" (required)
- `description`: Server description (required)
- `extra`: Additional information, JSON object (optional)

**Response**:
```json
{
  "id": "uuid",
  "name": "example-server",
  "tag": "1.0.0",
  "endpoint": "http://127.0.0.1:8080/sse",
  "transport_type": "sse",
  "description": "Example MCP server",
  "extra": {
    "custom_field": "value"
  },
  "disabled": false,
  "created_at": "2024-01-01T00:00:00",
  "updated_at": "2024-01-01T00:00:00",
  "deleted_at": null
}
```

### 3. Proxy Services

MCP Center provides reverse proxy functionality to forward client requests to the corresponding MCP servers.

#### SSE Connection Proxy

```http
GET /proxy/connect/{name}/{tag}
```

**Path Parameters**:
- `name`: MCP server name
- `tag`: Version tag

**Description**: This endpoint establishes an SSE (Server-Sent Events) connection with the specified MCP server.

#### Message Proxy

```http
POST /proxy/message/{name}/{tag}/{*subPath}
```

**Path Parameters**:
- `name`: MCP server name
- `tag`: Version tag
- `subPath`: Sub-path, supports wildcards

**Description**: This endpoint forwards messages to the corresponding path of the specified MCP server.

## Error Handling

The API uses standard HTTP status codes to indicate request results:

- `200 OK`: Request successful
- `400 Bad Request`: Invalid request parameters
- `401 Unauthorized`: Authentication failed
- `404 Not Found`: Resource not found
- `500 Internal Server Error`: Internal server error

**Error Response Format**:
```txt
Error description message
```

## Usage Examples

### 1. Register MCP Server

```bash
curl -X POST http://localhost:5432/api/registry/mcp-server \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-admin-token" \
  -d '{
    "name": "my-mcp-server",
    "tag": "1.0.0",
    "endpoint": "http://my-server:8080/sse",
    "transport_type": "sse",
    "description": "My MCP server"
  }'
```

### 2. Get All MCP Servers

```bash
curl -X GET http://localhost:5432/api/registry/mcp-server \
  -H "Authorization: Bearer your-admin-token"
```

### 3. Connect to MCP Server via Proxy

```bash
curl -X GET http://localhost:5432/proxy/connect/my-mcp-server/1.0.0 \
  -H "Authorization: Bearer your-api-key"
```

