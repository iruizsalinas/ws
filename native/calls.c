/* Calls that may return owned strings. Prepare the Lua converter first:
 * it can allocate and so run finalizers, which may replace arguments and
 * collect their strings. Then read arguments, enter Rust, and copy the
 * result under a protected call. */
#include "glue.h"

/* Fix the arity first, so the converter lands after every argument and an
 * omitted one stays nil instead of reading the converter. */
static void prepare_args(lua_State *L, int args, ws_str *s) {
    lua_settop(L, args);
    prepare_str(L, s);
}

int l_wait(lua_State *L) {
    uint64_t hub = check_id(L, 1);
    double timeout = luaL_optnumber(L, 2, -1);
    ws_str s;
    prepare_str(L, &s);
    ws_event ev;
    int rc = ws_wait(hub, timeout, &ev);
    if (rc < 0) return result(L, -rc, &s);
    if (rc == 0) return 0;
    s = ev.data;
    push_str(L, &s);
    lua_pushinteger(L, ev.kind);
    lua_pushinteger(L, (lua_Integer)ev.id);
    lua_pushvalue(L, -3); /* the protected conversion's string */
    lua_pushinteger(L, ev.code);
    lua_pushinteger(L, (lua_Integer)ev.server);
    return 5;
}

int l_connect(lua_State *L) {
    ws_str s;
    prepare_args(L, 9, &s);
    uint64_t hub = check_id(L, 1), id = 0;
    ws_str url = opt_str(L, 2), protocols = opt_str(L, 3), headers = opt_str(L, 4), ca = opt_str(L, 5);
    double timeout = luaL_optnumber(L, 6, 0);
    uint64_t max = opt_size(L, 7), buffer = opt_size(L, 8);
    double heartbeat = opt_heartbeat(L, 9);
    int rc = ws_connect(hub, url, protocols, headers, ca, timeout, max, buffer, heartbeat, &id);
    return handle(L, rc, id, &s);
}

int l_listen(lua_State *L) {
    ws_str s;
    prepare_args(L, 9, &s);
    uint64_t hub = check_id(L, 1), id = 0;
    ws_str host = opt_str(L, 2), cert = opt_str(L, 4), key = opt_str(L, 5);
    lua_Integer port = luaL_checkinteger(L, 3);
    luaL_argcheck(L, port >= 0 && port <= 65535, 3, "invalid port");
    uint64_t max = opt_size(L, 6), buffer = opt_size(L, 7), connections = opt_size(L, 8);
    double heartbeat = opt_heartbeat(L, 9);
    int rc = ws_listen(hub, host, (uint16_t)port, cert, key, max, buffer, connections, heartbeat, &id);
    return handle(L, rc, id, &s);
}

int l_port(lua_State *L) {
    lua_pushinteger(L, ws_port(check_id(L, 1)));
    return 1;
}

int l_unlisten(lua_State *L) {
    ws_unlisten(check_id(L, 1));
    return 0;
}

int l_reply(lua_State *L) {
    ws_str s;
    prepare_args(L, 3, &s);
    uint64_t token = check_id(L, 1);
    lua_Integer status = luaL_optinteger(L, 2, 0);
    luaL_argcheck(L, status >= 0 && status <= 65535, 2, "invalid status");
    ws_str protocol = opt_str(L, 3);
    return result(L, ws_reply(token, (uint16_t)status, protocol), &s);
}

int l_send(lua_State *L) {
    ws_str s;
    prepare_args(L, 3, &s);
    uint64_t id = check_id(L, 1);
    ws_str data = opt_str(L, 2);
    int binary = lua_toboolean(L, 3);
    return result(L, ws_send(id, data, binary), &s);
}

int l_ping(lua_State *L) {
    ws_str s;
    prepare_args(L, 2, &s);
    uint64_t id = check_id(L, 1);
    ws_str data = opt_str(L, 2);
    return result(L, ws_ping(id, data), &s);
}

int l_close(lua_State *L) {
    ws_str s;
    prepare_args(L, 3, &s);
    uint64_t id = check_id(L, 1);
    lua_Integer code = luaL_optinteger(L, 2, 1000);
    if (code < 0 || code > 65535) code = 0; /* rejected by ws_close */
    ws_str reason = opt_str(L, 3);
    return result(L, ws_close(id, (int)code, reason), &s);
}

int l_terminate(lua_State *L) {
    ws_terminate(check_id(L, 1));
    return 0;
}

int l_protocol(lua_State *L) {
    uint64_t id = check_id(L, 1);
    ws_str s;
    prepare_str(L, &s);
    s = ws_protocol(id);
    push_str(L, &s);
    return 1;
}
