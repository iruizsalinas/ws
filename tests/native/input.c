/* Run Lua code where the production glue prepares its converter, modelling
 * a GC finalizer there. Lua's debug API can replace a C function's
 * arguments and collect their strings, so the glue must prepare before it
 * reads any borrowed argument. */
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif
#include "../../native/glue.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void interrupted_prepare(lua_State *L, lua_CFunction fn, int upvalues);
#define lua_pushcclosure interrupted_prepare
#include "../../native/values.c"
#undef lua_pushcclosure
#include "../../native/calls.c"

static int armed, disturbed;
static ws_str seen;
static _Thread_local void *context;
void *ws_copy_context(void) { return context; }
void *ws_set_copy_context(void *next) {
    void *previous = context;
    context = next;
    return previous;
}
void ws_free(ws_str s) { free((void *)s.ptr); }
ws_str ws_error(void) { return (ws_str){NULL, 0}; }

/* The stubbed ABI copies what it is given, as the real one would. */
static int record(ws_str s) {
    free((void *)seen.ptr);
    seen = (ws_str){NULL, s.len};
    if (s.ptr) {
        char *copy = malloc(s.len ? s.len : 1);
        assert(copy);
        memcpy(copy, s.ptr, s.len);
        seen.ptr = copy;
    }
    return WS_OK;
}
int ws_send(uint64_t id, ws_str data, _Bool binary) { (void)id, (void)binary; return record(data); }
int ws_ping(uint64_t id, ws_str data) { (void)id; return record(data); }
int ws_close(uint64_t id, int code, ws_str reason) { (void)id, (void)code; return record(reason); }
int ws_reply(uint64_t token, uint16_t status, ws_str protocol) { (void)token, (void)status; return record(protocol); }

int ws_connect(uint64_t hub, ws_str url, ws_str protocols, ws_str headers, ws_str ca, double timeout, uint64_t max,
               uint64_t buffer, double heartbeat, uint64_t *id) {
    (void)hub, (void)protocols, (void)headers, (void)ca, (void)timeout, (void)max, (void)buffer, (void)heartbeat;
    *id = 1;
    return record(url);
}
int ws_listen(uint64_t hub, ws_str host, uint16_t port, ws_str cert, ws_str key, uint64_t max, uint64_t buffer,
              uint64_t connections, double heartbeat, uint64_t *id) {
    (void)hub, (void)port, (void)cert, (void)key, (void)max, (void)buffer, (void)connections, (void)heartbeat;
    *id = 1;
    return record(host);
}

/* Linked by calls.c but never reached here. */
int ws_wait(uint64_t hub, double timeout, ws_event *ev) { (void)hub, (void)timeout, (void)ev; abort(); }
uint16_t ws_port(uint64_t server) { (void)server; abort(); }
void ws_unlisten(uint64_t server) { (void)server; abort(); }
void ws_terminate(uint64_t id) { (void)id; abort(); }
ws_str ws_protocol(uint64_t id) { (void)id; abort(); }

/* Clear argument `slot` of the running `target` and collect its string. */
static void interrupted_prepare(lua_State *L, lua_CFunction fn, int upvalues) {
    if (armed) {
        armed = 0;
        const char *script =
            "for level = 1, 20 do"
            "  local info = assert(debug.getinfo(level, 'f'), 'target frame not found')"
            "  if info.func == target then debug.setlocal(level, slot, nil) break end "
            "end "
            "collectgarbage('collect')";
        assert(luaL_loadstring(L, script) == 0);
        assert(ws_pcall(L, 0, 0) == 0);
        disturbed = 1;
    }
    lua_pushcclosure(L, fn, upvalues);
}

static void run(lua_State *L, const char *chunk) {
    assert(luaL_loadstring(L, chunk) == 0);
    if (ws_pcall(L, 0, 0) != 0) {
        fprintf(stderr, "%s: %s\n", chunk, luaL_checklstring(L, -1, NULL));
        abort();
    }
}

/* `call` passes a fresh 64-byte string as argument `slot` of `target`. */
static void check(lua_State *L, const char *call) {
    run(L, call); /* undisturbed: the stub sees the argument */
    assert(seen.ptr && seen.len == 64);
    for (size_t i = 0; i < seen.len; i++) assert(seen.ptr[i] == 'x');
    armed = 1, disturbed = 0;
    run(L, call);
    assert(disturbed);
    /* Read after preparation, the cleared argument is absent, not freed. */
    assert(!seen.ptr);
}

#ifdef _WIN32
__declspec(dllexport)
#endif
int luaopen_ws_input(lua_State *L) {
#ifdef _WIN32
    HMODULE runtime;
    DWORD flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    assert(GetModuleHandleExA(flags, (LPCSTR)__builtin_return_address(0), &runtime));
#else
    void *runtime = NULL;
#endif
    assert(ws_bind_api(runtime));
    /* Globals through a chunk, since lua_setglobal differs across versions. */
    assert(luaL_loadstring(L, "connect, listen, reply, send, ping, close = ...") == 0);
    lua_CFunction calls[] = {l_connect, l_listen, l_reply, l_send, l_ping, l_close};
    for (int i = 0; i < 6; i++) lua_pushcclosure(L, calls[i], 0);
    assert(ws_pcall(L, 6, 0) == 0);
    check(L, "target, slot = connect, 2; connect(1, string.rep('x', 64))");
    check(L, "target, slot = listen, 2; listen(1, string.rep('x', 64), 0)");
    check(L, "target, slot = send, 2; send(1, string.rep('x', 64), true)");
    check(L, "target, slot = ping, 2; ping(1, string.rep('x', 64))");
    check(L, "target, slot = close, 3; close(1, 1000, string.rep('x', 64))");
    check(L, "target, slot = reply, 3; reply(1, 0, string.rep('x', 64))");
    record((ws_str){NULL, 0});
    lua_pushboolean(L, 1);
    return 1;
}
