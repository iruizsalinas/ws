/* Validation and error-safe copying of Rust-owned strings into Lua. */
#include "glue.h"

ws_str opt_str(lua_State *L, int i) {
    ws_str s = {NULL, 0};
    if (!lua_isnoneornil(L, i)) s.ptr = luaL_checklstring(L, i, &s.len);
    return s;
}

uint64_t check_id(lua_State *L, int i) {
    lua_Integer n = luaL_checkinteger(L, i);
    luaL_argcheck(L, n > 0, i, "invalid handle");
    return (uint64_t)n;
}

uint64_t opt_size(lua_State *L, int i) {
    lua_Number n = luaL_optnumber(L, i, 0);
    luaL_argcheck(L, n >= 0 && n <= 9007199254740992.0, i, "invalid size");
    uint64_t size = (uint64_t)n;
    luaL_argcheck(L, (lua_Number)size == n, i, "invalid size");
    return size;
}

/* nil keeps the default (-1), false disables (0), else positive seconds. */
double opt_heartbeat(lua_State *L, int i) {
    if (lua_isnoneornil(L, i)) return -1;
    if (lua_isboolean(L, i)) {
        luaL_argcheck(L, !lua_toboolean(L, i), i, "invalid heartbeat");
        return 0;
    }
    lua_Number n = luaL_checknumber(L, i);
    luaL_argcheck(L, n > 0 && n <= 1e6, i, "invalid heartbeat");
    return n;
}

/* Keep the owner in C, where Lua's debug API cannot replace it. Nested
 * conversions save and restore their own context on the same thread. */
struct Copy {
    lua_State *lua;
    ws_str *string;
    struct Copy *previous;
    int entered;
};
static int copy_str(lua_State *L) {
    struct Copy *current = ws_copy_context();
    if (!current || current->lua != L || current->entered) {
        lua_pushstring(L, "invalid string conversion");
        return lua_error(L);
    }
    current->entered = 1;
    const ws_str *s = current->string;
    if (s->ptr) lua_pushlstring(L, s->ptr, s->len);
    else lua_pushnil(L);
    return 1;
}

/* Creating a closure can allocate. Prepare before calling Rust, while
 * there is no owned string to lose if Lua raises. */
void prepare_str(lua_State *L, ws_str *s) {
    *s = (ws_str){NULL, 0};
    lua_pushcclosure(L, copy_str, 0);
}

/* The prepared function is on top. The stack-local owner
 * survives a protected copy, including nested finalizers and Lua errors. */
void push_str(lua_State *L, ws_str *s) {
    struct Copy copy = {L, s, NULL, 0};
    copy.previous = ws_set_copy_context(&copy);
    int rc = ws_pcall(L, 0, 1);
    ws_set_copy_context(copy.previous);
    ws_free(*s);
    *s = (ws_str){NULL, 0};
    if (rc != 0) lua_error(L);
}

/* true, or nil, message, fatal; only the Lua side raises native errors. */
int result(lua_State *L, int rc, ws_str *s) {
    if (rc == WS_OK) {
        lua_settop(L, -2); /* discard the unused converter */
        lua_pushboolean(L, 1);
        return 1;
    }
    *s = ws_error();
    push_str(L, s);
    lua_pushnil(L);
    lua_pushvalue(L, -2);
    lua_pushboolean(L, rc == WS_FATAL);
    return 3;
}

int handle(lua_State *L, int rc, uint64_t id, ws_str *s) {
    if (rc != WS_OK) return result(L, rc, s);
    lua_settop(L, -2);
    lua_pushinteger(L, (lua_Integer)id);
    return 1;
}
