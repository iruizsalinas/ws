/* The `ws.core` Lua module: converts values between Lua and the Rust ABI
 * (ws.h) and nothing more. It keeps no state and never calls into Lua, so
 * a Lua error can only unwind through this file. The API lives in ws/. */
#include <lauxlib.h>
#include <lua.h>

#include "ws.h"

#if LUA_VERSION_NUM == 501
#define newlib(L, f) (lua_newtable(L), luaL_register(L, NULL, f))
#else
#define newlib(L, f) luaL_newlib(L, f)
#endif

static const char *const KINDS[] = {
    NULL, "open", "message", "ping", "pong", "error", "close", "request", "connection",
};
static const char *const STATES[] = {"connecting", "open", "closing", "closed"};

static ws_str opt_str(lua_State *L, int i) {
    ws_str s = {NULL, 0};
    if (!lua_isnoneornil(L, i)) s.ptr = luaL_checklstring(L, i, &s.len);
    return s;
}

static uint64_t check_id(lua_State *L, int i) {
    lua_Integer n = luaL_checkinteger(L, i);
    luaL_argcheck(L, n > 0, i, "invalid handle");
    return (uint64_t)n;
}

static void push_str(lua_State *L, ws_str s) {
    if (s.ptr) lua_pushlstring(L, s.ptr, s.len);
    else lua_pushnil(L);
}

/* `true`, or `nil, message, fatal`: the Lua side raises fatal ones. */
static int result(lua_State *L, int rc) {
    if (rc == WS_OK) {
        lua_pushboolean(L, 1);
        return 1;
    }
    lua_pushnil(L);
    push_str(L, ws_error());
    lua_pushboolean(L, rc == WS_FATAL);
    return 3;
}

static int handle(lua_State *L, int rc, uint64_t id) {
    if (rc != WS_OK) return result(L, rc);
    lua_pushinteger(L, (lua_Integer)id);
    return 1;
}

static int l_hub(lua_State *L) {
    lua_pushinteger(L, (lua_Integer)ws_hub());
    return 1;
}

static int l_now(lua_State *L) {
    lua_pushnumber(L, ws_now());
    return 1;
}

/* kind, id, data, code, server; nothing on timeout. */
static int l_wait(lua_State *L) {
    ws_event ev;
    int rc = ws_wait(check_id(L, 1), luaL_optnumber(L, 2, -1), &ev);
    if (rc < 0) return result(L, -rc);
    if (rc == 0) return 0;
    luaL_argcheck(L, ev.kind > 0 && ev.kind <= 8, 1, "unknown event");
    lua_pushstring(L, KINDS[ev.kind]);
    lua_pushinteger(L, (lua_Integer)ev.id);
    push_str(L, ev.data);
    lua_pushinteger(L, ev.code);
    lua_pushinteger(L, (lua_Integer)ev.server);
    return 5;
}

static int l_connect(lua_State *L) {
    uint64_t id = 0;
    int rc = ws_connect(check_id(L, 1), opt_str(L, 2), opt_str(L, 3), opt_str(L, 4), opt_str(L, 5),
                        luaL_optnumber(L, 6, 0), (uint64_t)luaL_optinteger(L, 7, 0), &id);
    return handle(L, rc, id);
}

static int l_listen(lua_State *L) {
    uint64_t id = 0;
    lua_Integer port = luaL_checkinteger(L, 3);
    luaL_argcheck(L, port >= 0 && port <= 65535, 3, "invalid port");
    int rc = ws_listen(check_id(L, 1), opt_str(L, 2), (uint16_t)port, opt_str(L, 4), opt_str(L, 5),
                       (uint64_t)luaL_optinteger(L, 6, 0), &id);
    return handle(L, rc, id);
}

static int l_port(lua_State *L) {
    lua_pushinteger(L, ws_port(check_id(L, 1)));
    return 1;
}

static int l_unlisten(lua_State *L) {
    ws_unlisten(check_id(L, 1));
    return 0;
}

static int l_reply(lua_State *L) {
    lua_Integer status = luaL_optinteger(L, 2, 0);
    luaL_argcheck(L, status >= 0 && status <= 65535, 2, "invalid status");
    return result(L, ws_reply(check_id(L, 1), (uint16_t)status, opt_str(L, 3)));
}

static int l_send(lua_State *L) {
    return result(L, ws_send(check_id(L, 1), opt_str(L, 2), lua_toboolean(L, 3)));
}

static int l_ping(lua_State *L) {
    return result(L, ws_ping(check_id(L, 1), opt_str(L, 2)));
}

static int l_close(lua_State *L) {
    lua_Integer code = luaL_optinteger(L, 2, 1000);
    if (code < 0 || code > 65535) code = 0; /* rejected by ws_close */
    return result(L, ws_close(check_id(L, 1), (int)code, opt_str(L, 3)));
}

static int l_terminate(lua_State *L) {
    ws_terminate(check_id(L, 1));
    return 0;
}

static int l_state(lua_State *L) {
    lua_pushstring(L, STATES[ws_state(check_id(L, 1)) & 3]);
    return 1;
}

static int l_protocol(lua_State *L) {
    push_str(L, ws_protocol(check_id(L, 1)));
    return 1;
}

static int l_buffered(lua_State *L) {
    lua_pushinteger(L, (lua_Integer)ws_buffered(check_id(L, 1)));
    return 1;
}

static const luaL_Reg FUNCS[] = {
    {"hub", l_hub},         {"now", l_now},         {"wait", l_wait},   {"connect", l_connect},
    {"listen", l_listen},   {"port", l_port},       {"unlisten", l_unlisten}, {"reply", l_reply},
    {"send", l_send},       {"ping", l_ping},       {"close", l_close}, {"terminate", l_terminate},
    {"state", l_state},     {"protocol", l_protocol}, {"buffered", l_buffered}, {NULL, NULL},
};

/* Called from luaopen_ws_core, which Rust exports. */
int ws_open(lua_State *L) {
    newlib(L, FUNCS);
    return 1;
}
