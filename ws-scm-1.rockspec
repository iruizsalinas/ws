rockspec_format = "3.0"
package = "ws"
version = "scm-1"
source = {
  url = "git+https://github.com/iruizsalinas/ws.git",
}
description = {
  summary = "A WebSocket client and server library for Lua",
  detailed = "RFC 6455 WebSocket client and server with TLS, running on a Rust core. Prebuilt for common platforms.",
  homepage = "https://github.com/iruizsalinas/ws",
  license = "MIT",
  labels = { "websocket", "networking", "tls" },
}
dependencies = { "lua >= 5.1, < 5.6" }
build = {
  type = "command",
  build_command = '"$(LUA)" scripts/native.lua "$(LUA_INCDIR)" "$(LUA_LIBDIR)" "$(LUA_BINDIR)" "$(LUALIB)"',
  install = {
    lua = {
      ["ws"] = "ws/init.lua",
      ["ws.emitter"] = "ws/emitter.lua",
      ["ws.loop"] = "ws/loop.lua",
      ["ws.opts"] = "ws/opts.lua",
      ["ws.server"] = "ws/server.lua",
      ["ws.socket"] = "ws/socket.lua",
      ["ws.timer"] = "ws/timer.lua",
    },
    lib = { ["ws.core"] = "ws/core.so" },
  },
  platforms = {
    windows = { install = { lib = { ["ws.core"] = "ws/core.dll" } } },
  },
}
