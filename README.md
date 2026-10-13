# ws

A WebSocket client and server library for Lua.

## Installation

```
luarocks install ws
```

Supports Lua 5.1 to 5.5 and LuaJIT. TLS (`wss://`) is included.

## Quick Start

### Client

```lua
local ws = require("ws")

local client = ws.connect("ws://localhost:8080")

client:on("open", function()
  client:send("hello")
end)

client:on("message", function(data, binary)
  print("received:", data)
  client:close()
end)

client:on("close", function(code, reason)
  print("closed:", code, reason)
end)

ws.run()
```

### Server

```lua
local ws = require("ws")

local server = ws.listen(8080)

server:on("connection", function(client, req)
  print("client connected:", req.path)

  client:on("message", function(data, binary)
    client:send(data, binary)
  end)
end)

ws.run()
```

## API Reference

### Functions

| Function | Description |
|---|---|
| `ws.connect(url, options)` | Create a client and start connecting |
| `ws.listen(port)` / `ws.listen(options)` | Start a server; raises an error if the port cannot be bound |
| `ws.run()` | Run until all clients, servers and timers are closed |
| `ws.poll(timeout)` | Process pending events without blocking longer than `timeout`; returns `true` while anything is active |
| `ws.after(seconds, fn)` | Run `fn` once after a delay; returns a timer with `:cancel()` |
| `ws.every(seconds, fn)` | Run `fn` repeatedly; returns a timer with `:cancel()` |

### WebSocket

| Method | Description |
|---|---|
| `:send(data, binary)` | Send a text message, or binary if `binary` is `true` |
| `:ping(data)` | Send a ping |
| `:close(code, reason)` | Close the connection |
| `:terminate()` | Drop the connection immediately |
| `:on(event, fn)` | Register an event listener |

**Properties:** `state`, `protocol`, `url`, `buffered`

**Events:** `open`, `message`, `ping`, `pong`, `close`, `error`

### Server

| Method | Description |
|---|---|
| `:close()` | Stop the server and close its connections |
| `:on(event, fn)` | Register an event listener |

**Properties:** `port`

**Events:** `connection` with `(client, req)`, where `req` has `path`, `headers`, `address`, `port` and `protocols`

### Client Options

| Option | Default | Description |
|---|---|---|
| `protocols` | `nil` | Subprotocol or list of subprotocols |
| `headers` | `nil` | Extra request headers |
| `ca` | `nil` | Path to a PEM file of trusted certificates |
| `timeout` | `10` | Connection timeout in seconds |
| `limit` | `1048576` | Maximum message size in bytes (1 MiB) |

### Server Options

| Option | Default | Description |
|---|---|---|
| `port` | Required | Bind port; use `0` to choose an available port |
| `host` | `"0.0.0.0"` | Bind address |
| `protocols` | `nil` | Supported subprotocols |
| `accept` | `nil` | `function(req)` returning `true` to accept, or `false, status` to reject |
| `cert` | `nil` | Path to a PEM certificate, to serve `wss://` |
| `key` | `nil` | Path to the PEM private key |
| `limit` | `1048576` | Maximum message size in bytes (1 MiB) |

## License

MIT
