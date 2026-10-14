/* ws.core: Lua module registration and calls without returned strings.
 * Calls into Rust finish before using Lua. The Unix load bridge permits
 * LuaJIT's external unwinding and has no destructors. */
#include "glue.h"

static const char *const STATES[] = {"connecting", "open", "closing", "closed"};

static int l_hub(lua_State *L) {
    int owned = !lua_isnoneornil(L, 1);
    if (owned) {
        luaL_argcheck(L, lua_type(L, 1) == LUA_TTABLE, 1, "invalid hub owner");
        lua_pushstring(L, "hub");
        lua_pushvalue(L, -1);
        lua_rawget(L, 1);
        luaL_argcheck(L, lua_isboolean(L, -1) && !lua_toboolean(L, -1), 1, "invalid hub owner");
        lua_settop(L, -2);
    }
    uint64_t id = ws_hub();
    lua_pushinteger(L, (lua_Integer)id);
    if (owned) {
        /* The key already exists: rawset cannot allocate or call __newindex.
         * Record ownership before Lua can run a failing return hook. */
        lua_rawset(L, 1);
        lua_pushinteger(L, (lua_Integer)id);
    }
    return 1;
}

static int l_shutdown(lua_State *L) {
    ws_shutdown(check_id(L, 1));
    return 0;
}

static int l_now(lua_State *L) {
    lua_pushnumber(L, ws_now());
    return 1;
}

static int l_state(lua_State *L) {
    lua_pushstring(L, STATES[ws_state(check_id(L, 1)) & 3]);
    return 1;
}

static int l_buffered(lua_State *L) {
    lua_pushinteger(L, (lua_Integer)ws_buffered(check_id(L, 1)));
    return 1;
}

static const struct {
    const char *name;
    lua_CFunction fn;
} FUNCS[] = {
    {"hub", l_hub},             {"shutdown", l_shutdown}, {"now", l_now},
    {"wait", l_wait},           {"connect", l_connect},   {"listen", l_listen},
    {"port", l_port},           {"unlisten", l_unlisten}, {"reply", l_reply},
    {"send", l_send},           {"ping", l_ping},         {"close", l_close},
    {"terminate", l_terminate}, {"state", l_state},       {"protocol", l_protocol},
    {"buffered", l_buffered},
};

/* Read back without calling a number API: Lua 5.1 can have narrower integers. */
static int wide_integers(lua_State *L) {
    lua_pushinteger(L, (lua_Integer)1 << 40);
    const char *s = luaL_checklstring(L, -1, NULL), *want = "1099511627776";
    while (*s && *s == *want) s++, want++;
    int wide = *s == *want;
    lua_settop(L, -2);
    return wide;
}

int ws_open(lua_State *L) {
#ifndef _WIN32
    if (!ws_bind_api(NULL)) {
        lua_pushstring(L, "ws.core is not compatible with this Lua runtime");
        return lua_error(L);
    }
#endif
    if (!wide_integers(L)) {
        lua_pushstring(L, "ws.core needs a Lua with 64-bit integers");
        return lua_error(L);
    }
    /* Direct require("ws.core") must also reject incompatible number
     * types before calling any C API that takes or returns lua_Number. */
    if (luaL_loadstring(L, "return 2^53+1==2^53 and 2^24+1~=2^24") != 0 || ws_pcall(L, 0, 1) != 0)
        return lua_error(L);
    int doubles = lua_toboolean(L, -1);
    lua_settop(L, -2);
    if (!doubles) {
        lua_pushstring(L, "ws.core needs a Lua with double-precision numbers");
        return lua_error(L);
    }
    int n = (int)(sizeof FUNCS / sizeof FUNCS[0]);
    lua_createtable(L, 0, n);
    for (int i = 0; i < n; i++) {
        lua_pushcclosure(L, FUNCS[i].fn, 0);
        lua_setfield(L, -2, FUNCS[i].name);
    }
    return 1;
}

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
/* The caller's module must supply the same API on every invocation. */
__declspec(dllexport) int luaopen_ws_core(lua_State *L) {
    DWORD flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    HMODULE runtime;
    if (!GetModuleHandleExA(flags, (LPCSTR)__builtin_return_address(0), &runtime)) return 0;
    if (!ws_bind_api(runtime)) return 0;
    return ws_open(L);
}
#endif
