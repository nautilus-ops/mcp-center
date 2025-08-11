# Quick Start

## 📋 Table of Contents

- [Quick Deployment](#quick-deployment)
- [Configure MCP Servers](#configure-mcp-servers)
- [Connect to MCP Servers](#connect-to-mcp-servers)

## 🚀 Quick Deployment

### Method 1: Using Docker (Recommended)

```bash
# 1. Pull the latest image
docker pull nautilusops/mcp-center:latest

# 2. Create configuration file
cat > mcp_servers.toml << EOF
[[mcp_servers]]
endpoint = "http://127.0.0.1:8080/sse"
name = "example-server"
tag = "1.0.0"

[[mcp_servers]]
endpoint = "http://127.0.0.1:8888/mcp"
name = "example-server"
tag = "2.0.0"
EOF

# 3. Start the container
docker run -d \
  --name mcp-center \
  -p 5432:5432 \
  -v $(pwd)/mcp_servers.toml:/app/mcp_servers.toml \
  nautilusops/mcp-center:latest
```

### Method 2: Using Helm (Kubernetes)

```bash
# 1. Clone the repository
git clone https://github.com/your-org/mcp-center.git
cd mcp-center

# 2. Install MCP Center
helm install mcp-center .helm/mcp-center

# 3. Verify deployment
kubectl get pods -l app=mcp-center
```

### Method 3: Build from Source

```bash
# 1. Clone the repository
git clone https://github.com/your-org/mcp-center.git
cd mcp-center

# 2. Build the project
cargo build --release

# 3. Run the application
./target/release/mcp-center run --config bootstrap.toml
```

## ⚙️ Configure MCP Servers

MCP Center supports two registration methods: local memory configuration and external API.

### Local Memory Configuration

1. **Create configuration file** `mcp_servers.toml`:

```toml
# Example MCP server configuration
[[mcp_servers]]
endpoint = "http://127.0.0.1:8080/sse"
name = "example-server"
tag = "1.0.0"
transport_type = "sse"
# Access URL: http://{proxy-host}:5432/connect/example-server/1.0.0

[[mcp_servers]]
endpoint = "http://127.0.0.1:8888/mcp"
name = "example-server"
tag = "2.0.0"
transport_type = "streamable"
# Access URL: http://{proxy-host}:5432/connect/example-server/2.0.0

[[mcp_servers]]
endpoint = "http://another-server:8080/sse"
name = "production-server"
tag = "stable"
transport_type = "sse"
# Access URL: http://{proxy-host}:5432/connect/production-server/stable
```

2. **Set environment variables**:

```bash
export REGISTRY_TYPE=memory
```

### External API Configuration

1. **Set environment variables**:

```bash
export REGISTRY_TYPE=external_api
export EXTERNAL_API=https://your-registry-api.com/api/mcp/servers
export EXTERNAL_AUTHORIZATION=your-api-token
```

2. **API Response Format**:

Your API must return JSON responses in the following format:

```json
{
    "data": [
        {
            "endpoint": "http://127.0.0.1:8080/sse",
            "name": "example-server",
            "version": "1.0.0",
            "tag": "1.0.0",
            "transport_type": "sse"
        },
        {
            "endpoint": "http://another-server:8080/mcp",
            "name": "production-server",
            "tag": "stable",
            "transport_type": "streamable"
        }
    ]
}
```

> **Note**: When both `version` and `tag` fields exist, the `tag` field takes precedence.

## 🔗 Connect to MCP Servers

### Go Language Example

```go
package main

import (
    "log"
    "github.com/modelcontextprotocol/go-sdk/mcp"
)

func main() {
    // Using SSE transport
    transport := mcp.NewSSEClientTransport(
        "http://localhost:5432/connect/example-server/1.0.0",
        nil,
    )
    
    // Using Streamable transport
    transport := mcp.NewStreamableClientTransport(
        "http://localhost:5432/connect/example-server/2.0.0",
        nil,
    )
    
    // Create client
    client := mcp.NewClient(&mcp.Implementation{
		Name:    "mcp-client",
		Version: "1.0.0",
	}, nil)
    
    // Initialize connection
    session, err := client.Connect(context.Background(), transport)
	if err != nil {
		panic(err)
	}
    
    defer session.Close()
    
    // Use the client...
}
```

### Direct HTTP Access

```bash
# Test connection
curl -v http://localhost:5432/connect/example-server/1.0.0
```