/* The `ws.core` Lua module: converts values between Lua and the Rust ABI
 * (ws.h) and nothing more. It never calls into Lua, so a Lua error can only
 * unwind through this file. The API lives in ws/.
 *
 * One build serves every Lua version (see luaapi.h). On Windows a DLL would
 * be tied to one Lua DLL name, so the API is looked up at load time in the
 * module that called luaopen_ws_core. */
#ifndef _WIN32
#define _GNU_SOURCE /* dladdr */
#include <dlfcn.h>
#endif
#include "luaapi.h"
#include "ws.h"

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#define X(ret, name, args) static ret(*name) args;
WS_LUA_API(X)
#undef X

static INIT_ONCE once = INIT_ONCE_STATIC_INIT;

static BOOL CALLBACK bind_api(PINIT_ONCE o, PVOID lua, PVOID *bound) {
    (void)o;
#define X(ret, name, args) \
    if (!(name = (ret(*) args)(void (*)(void))GetProcAddress(lua, #name))) return FALSE;
    WS_LUA_API(X)
#undef X
    *bound = lua;
    return TRUE;
}
#else
#define X(ret, name, args) ret name args;
WS_LUA_API(X)
#undef X
#endif

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

static uint64_t opt_size(lua_State *L, int i) {
    /* Avoid lua_Integer: Lua 5.1 builds may use a 32-bit integer even when
     * their lua_Number can exactly represent much larger size limits. */
    lua_Number n = luaL_optnumber(L, i, 0);
    luaL_argcheck(L, n >= 0 && n <= 9007199254740992.0, i, "invalid size");
    uint64_t size = (uint64_t)n;
    luaL_argcheck(L, (lua_Number)size == n, i, "invalid size");
    return size;
}

/* nil keeps the default (-1), false disables (0), else positive seconds. */
static double opt_heartbeat(lua_State *L, int i) {
    if (lua_isnoneornil(L, i)) return -1;
    if (lua_isboolean(L, i)) {
        luaL_argcheck(L, !lua_toboolean(L, i), i, "invalid heartbeat");
        return 0;
    }
    lua_Number n = luaL_checknumber(L, i);
    luaL_argcheck(L, n > 0 && n <= 1e6, i, "invalid heartbeat");
    return n;
}

/* Copies a returned string into Lua, then releases it. Should the copy raise
 * (out of memory, or a finalizer error in Lua 5.1), the string leaks: no
 * protected call exists in every Lua version, and leaking is safe. */
static void push_str(lua_State *L, ws_str s) {
    if (s.ptr) lua_pushlstring(L, s.ptr, s.len);
    else lua_pushnil(L);
    ws_free(s);
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

static int l_shutdown(lua_State *L) {
    ws_shutdown(check_id(L, 1));
    return 0;
}

static int l_now(lua_State *L) {
    lua_pushnumber(L, ws_now());
    return 1;
}

/* kind (a number, see loop.kinds), id, data, code, server; nothing on
 * timeout. Numbers never raise, so only copying data can lose it. */
static int l_wait(lua_State *L) {
    ws_event ev;
    int rc = ws_wait(check_id(L, 1), luaL_optnumber(L, 2, -1), &ev);
    if (rc < 0) return result(L, -rc);
    if (rc == 0) return 0;
    lua_pushinteger(L, ev.kind);
    lua_pushinteger(L, (lua_Integer)ev.id);
    push_str(L, ev.data);
    lua_pushinteger(L, ev.code);
    lua_pushinteger(L, (lua_Integer)ev.server);
    return 5;
}

static int l_connect(lua_State *L) {
    uint64_t id = 0;
    int rc = ws_connect(check_id(L, 1), opt_str(L, 2), opt_str(L, 3), opt_str(L, 4), opt_str(L, 5),
                        luaL_optnumber(L, 6, 0), opt_size(L, 7), opt_size(L, 8), opt_heartbeat(L, 9), &id);
    return handle(L, rc, id);
}

static int l_listen(lua_State *L) {
    uint64_t id = 0;
    lua_Integer port = luaL_checkinteger(L, 3);
    luaL_argcheck(L, port >= 0 && port <= 65535, 3, "invalid port");
    int rc = ws_listen(check_id(L, 1), opt_str(L, 2), (uint16_t)port, opt_str(L, 4), opt_str(L, 5),
                       opt_size(L, 6), opt_size(L, 7), opt_size(L, 8), opt_heartbeat(L, 9), &id);
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

/* Whether lua_Integer is 64 bits, as luaapi.h assumes: a narrower one
 * truncates 2^40. Read back as a string because calls involving lua_Number
 * are unsafe until its type is known (ws/loop.lua checks that, in Lua). */
static int wide_integers(lua_State *L) {
    lua_pushinteger(L, (lua_Integer)1 << 40);
    const char *s = luaL_checklstring(L, -1, NULL), *want = "1099511627776";
    while (*s && *s == *want) s++, want++;
    lua_settop(L, -2);
    return *s == *want;
}

/* Lua unloads C modules when a state closes, but ws threads may still be
 * finishing (ws_shutdown only starts that), so the module stays loaded. */
static void pin(void) {
#ifdef _WIN32
    HMODULE self;
    DWORD flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN;
    (void)GetModuleHandleExA(flags, (LPCSTR)&once, &self);
#else
    Dl_info info;
    if (dladdr((const void *)STATES, &info) && info.dli_fname)
        (void)dlopen(info.dli_fname, RTLD_NOW | RTLD_NOLOAD | RTLD_NODELETE);
#endif
}

int ws_open(lua_State *L) {
    if (!wide_integers(L)) {
        lua_pushstring(L, "ws.core needs a Lua with 64-bit integers");
        return lua_error(L);
    }
    pin();
    int n = (int)(sizeof FUNCS / sizeof FUNCS[0]);
    lua_createtable(L, 0, n);
    for (int i = 0; i < n; i++) {
        lua_pushcclosure(L, FUNCS[i].fn, 0);
        lua_setfield(L, -2, FUNCS[i].name);
    }
    return 1;
}

#ifdef _WIN32
/* Exported from C, not Rust, so the return address is in the Lua runtime.
 * Without its API, or when bound to another runtime, this returns nothing
 * and ws/loop.lua reports it: those pointers must never see this state. */
__declspec(dllexport) int luaopen_ws_core(lua_State *L) {
    DWORD flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    HMODULE lua;
    PVOID bound;
    if (!GetModuleHandleExA(flags, (LPCSTR)__builtin_return_address(0), &lua)) return 0;
    if (!InitOnceExecuteOnce(&once, bind_api, lua, &bound) || bound != lua) return 0;
    return ws_open(L);
}
#endif
