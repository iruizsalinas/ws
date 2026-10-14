/* The part of the Lua C API the glue uses. These functions have the same
 * signatures in Lua 5.1 to 5.5 and LuaJIT, so declaring them here instead
 * of including a version's headers lets one build serve all of them.
 *
 * Needs 64-bit integers and double numbers, the default on 64-bit
 * platforms; ws/loop.lua and ws_open refuse any other Lua. The string
 * pushes return a pointer since 5.2 and rawget a tag since 5.3; we ignore
 * those results, which does not change their call ABI. */
#ifndef WS_LUAAPI_H
#define WS_LUAAPI_H

#include <stddef.h>
#include <stdint.h>

typedef struct lua_State lua_State;
typedef int64_t lua_Integer;
typedef double lua_Number;
typedef int (*lua_CFunction)(lua_State *L);

#define WS_LUA_API(X)                                                      \
    X(int, lua_type, (lua_State * L, int i))                               \
    X(int, lua_toboolean, (lua_State * L, int i))                          \
    X(void, lua_settop, (lua_State * L, int i))                            \
    X(void, lua_pushvalue, (lua_State * L, int i))                         \
    X(void, lua_pushnil, (lua_State * L))                                  \
    X(void, lua_pushboolean, (lua_State * L, int b))                       \
    X(void, lua_pushinteger, (lua_State * L, lua_Integer n))               \
    X(void, lua_pushnumber, (lua_State * L, lua_Number n))                 \
    X(void, lua_pushlstring, (lua_State * L, const char *s, size_t len))   \
    X(void, lua_pushstring, (lua_State * L, const char *s))                \
    X(void, lua_pushcclosure, (lua_State * L, lua_CFunction f, int n))     \
    X(void, lua_createtable, (lua_State * L, int narr, int nrec))          \
    X(void, lua_setfield, (lua_State * L, int i, const char *k))           \
    X(void, lua_rawget, (lua_State * L, int i))                           \
    X(void, lua_rawset, (lua_State * L, int i))                           \
    X(int, lua_error, (lua_State * L))                                     \
    X(int, luaL_loadstring, (lua_State * L, const char *s))                 \
    X(const char *, luaL_checklstring, (lua_State * L, int i, size_t *n))  \
    X(lua_Number, luaL_checknumber, (lua_State * L, int i))                \
    X(lua_Number, luaL_optnumber, (lua_State * L, int i, lua_Number d))    \
    X(lua_Integer, luaL_checkinteger, (lua_State * L, int i))              \
    X(lua_Integer, luaL_optinteger, (lua_State * L, int i, lua_Integer d)) \
    X(int, luaL_argerror, (lua_State * L, int i, const char *msg))

#ifdef _WIN32
#define X(ret, name, args) extern ret (*name) args;
#else
#define X(ret, name, args) extern ret name args;
#endif
WS_LUA_API(X)
#undef X

/* Resolve the version-specific protected-call ABI and pin the module.
 * On Windows `runtime` is the module that invoked luaopen_ws_core. */
int ws_bind_api(void *runtime);
int ws_pcall(lua_State *L, int nargs, int nresults);

#define LUA_TNIL 0
#define LUA_TBOOLEAN 1
#define LUA_TNUMBER 3
#define LUA_TSTRING 4
#define LUA_TTABLE 5

#define lua_isnoneornil(L, i) (lua_type(L, i) <= LUA_TNIL)
#define lua_isboolean(L, i) (lua_type(L, i) == LUA_TBOOLEAN)
#define luaL_argcheck(L, cond, i, msg) ((void)((cond) || luaL_argerror(L, i, msg)))

#endif
