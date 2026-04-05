# ws

A WebSocket client and server library for Lua.

## Installation

```
luarocks install ws
```

This library requires [luasocket](https://github.com/lunarmodules/luasocket) for TCP networking. On most systems you also need a C compiler to build it.

**Linux (Ubuntu/Debian):**
```bash
sudo apt install -y lua5.4 liblua5.4-dev luarocks libssl-dev
luarocks install luasocket
luarocks install luasec    # optional, for wss://
```

**macOS:**
```bash
brew install lua luarocks openssl
luarocks install luasocket
luarocks install luasec OPENSSL_DIR=$(brew --prefix openssl)  # optional
```

**Windows:**

The easiest path is WSL. Run `wsl --install -d Ubuntu` in PowerShell, then follow the Linux instructions above.

## Dependencies

| Package | Required? | Purpose |
|---|---|---|
| [luasocket](https://luarocks.org/modules/luasocket/luasocket) | **Yes** | TCP networking |
| [luasec](https://luarocks.org/modules/brunoos/luasec) | No | TLS/SSL for `wss://` connections |
| [lua-zlib](https://luarocks.org/modules/brimworks/lua-zlib) | No | permessage-deflate compression |

LuaSocket is required for any network I/O. luasec and lua-zlib are detected at runtime. Without them, `wss://` and compression are unavailable, but everything else works. The core protocol logic (framing, SHA-1, base64, UTF-8 validation) is pure Lua.

## Quick Start

### Client

```lua
local ws = require("ws")

local client = ws.client("ws://localhost:8080")

client:on("open", function()
  client:send("hello")
end)

client:on("message", function(data, is_binary)
  print("received:", data)
end)

client:on("close", function(code, reason)
  print("closed:", code, reason)
end)

client:on("error", function(err)
  print("error:", err)
end)

client:connect()

-- poll for events in your main loop
while client.ready_state ~= "CLOSED" do
  client:poll(0.1)
end
```

### Server

```lua
local ws = require("ws")

local server = ws.server({ port = 8080 })

server:on("connection", function(client, request)
  print("client connected:", request.path)

  client:on("message", function(data, is_binary)
    client:send(data) -- echo
  end)

  client:on("close", function(code, reason)
    print("client disconnected:", code)
  end)
end)

server:listen(function()
  print("listening on port 8080")
end)

-- poll for connections and data
while true do
  server:poll(0.1)
end
```

### Binary Data

```lua
client:send(string.char(0x00, 0x01, 0x02, 0x03), { binary = true })

client:on("message", function(data, is_binary)
  if is_binary then
    local bytes = { string.byte(data, 1, #data) }
  end
end)
```

### Compression

Requires [lua-zlib](https://luarocks.org/modules/brimworks/lua-zlib).

```lua
-- client with permessage-deflate
local client = ws.client("ws://localhost:8080", {
  per_message_deflate = true,
})

-- server with permessage-deflate
local server = ws.server({
  port = 8080,
  per_message_deflate = {
    server_no_context_takeover = true,
    client_no_context_takeover = true,
    threshold = 1024, -- only compress messages >= 1024 bytes
  },
})
```

### TLS (wss://)

Requires [luasec](https://luarocks.org/modules/brunoos/luasec).

```lua
local client = ws.client("wss://echo.example.com", {
  tls = {
    verify = "peer",
    protocol = "any",
    cafile = "/etc/ssl/certs/ca-certificates.crt",
  },
})
client:connect()
```

### Ping/Pong

```lua
client:ping("heartbeat")

client:on("pong", function(data)
  print("pong received:", data)
end)
```

Auto-pong is enabled by default. To disable it:

```lua
local client = ws.client(url, { auto_pong = false })
```

### Subprotocols

```lua
local client = ws.client("ws://localhost:8080", {
  protocols = { "chat", "json" },
})

client:on("open", function()
  print("negotiated protocol:", client.protocol)
end)
```

## API Reference

### `ws.client(url, options) -> client`

Create a WebSocket client. Call `client:connect()` to connect.

### `ws.server(options) -> server`

Create a WebSocket server. Call `server:listen()` to start.

### `ws.connect(url, options) -> client, err`

Create and connect in one call. Returns a connected WebSocket, or nil and an error message.

### WebSocket

| Method | Description |
|---|---|
| `:connect()` | Connect to the server (blocking) |
| `:send(data, options, cb)` | Send a message |
| `:ping(data, cb)` | Send a ping frame |
| `:pong(data, cb)` | Send a pong frame |
| `:close(code, reason)` | Initiate close handshake |
| `:terminate()` | Forcibly close the connection |
| `:poll(timeout)` | Read and process incoming data |
| `:on(event, fn)` | Register an event listener |
| `:once(event, fn)` | Register a one-time listener |
| `:off(event, fn)` | Remove a listener |

**Properties:** `ready_state`, `protocol`, `extensions`, `url`

**Events:** `open`, `message`, `close`, `error`, `ping`, `pong`, `redirect`

### Server

| Method | Description |
|---|---|
| `:listen(callback)` | Start listening for connections |
| `:poll(timeout)` | Accept connections and read client data |
| `:close(callback)` | Graceful shutdown |
| `:address()` | Get bound address and port |
| `:on(event, fn)` | Register an event listener |

**Properties:** `clients` (table of connected WebSocket instances)

**Events:** `listening`, `connection`, `headers`, `close`, `error`

### Client Options

| Option | Default | Description |
|---|---|---|
| `protocols` | `{}` | Subprotocol names to negotiate |
| `headers` | `{}` | Custom HTTP headers |
| `max_payload` | `104857600` | Maximum message size (100MB) |
| `auto_pong` | `true` | Automatically respond to pings |
| `close_timeout` | `30` | Seconds to wait for close handshake |
| `handshake_timeout` | `30` | Connection timeout in seconds |
| `follow_redirects` | `false` | Follow HTTP redirects |
| `max_redirects` | `10` | Maximum redirect count |
| `per_message_deflate` | `true` | Enable compression (if lua-zlib available) |
| `skip_utf8_validation` | `false` | Skip UTF-8 validation on text messages |
| `origin` | `nil` | Value of the Origin header |
| `tls` | `{}` | LuaSec TLS options (for wss://) |

### Server Options

| Option | Default | Description |
|---|---|---|
| `host` | `"0.0.0.0"` | Bind address |
| `port` | required | Bind port |
| `backlog` | `511` | Connection backlog |
| `max_payload` | `104857600` | Maximum message size (100MB) |
| `auto_pong` | `true` | Automatically respond to pings |
| `close_timeout` | `30` | Close handshake timeout |
| `client_tracking` | `true` | Track connected clients |
| `per_message_deflate` | `false` | Enable compression |
| `path` | `nil` | Accept only connections matching this path |
| `verify_client` | `nil` | `function(info) -> bool` for auth |
| `handle_protocols` | `nil` | `function(protocols, headers) -> string` |
| `no_server` | `false` | Disable built-in TCP server |
| `skip_utf8_validation` | `false` | Skip UTF-8 validation |

## Compatibility

| Runtime | Status |
|---|---|
| Lua 5.1 | Supported |
| Lua 5.2 | Supported |
| Lua 5.3 | Supported |
| Lua 5.4 | Supported |
| Lua 5.5 | Supported |
| LuaJIT 2.1 | Supported (optimized) |

Platforms: Linux, macOS, Windows, any platform supported by LuaSocket.