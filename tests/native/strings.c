/* Exercise the production converter with real Lua protected calls and
 * allocator failures. The hook models a GC callback at string creation. */
#ifndef _WIN32
#define _GNU_SOURCE
#include <dlfcn.h>
#else
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif
#include "../../native/glue.h"
#include <assert.h>
#include <stdlib.h>
#include <string.h>

static void interrupted_copy(lua_State *L, const char *p, size_t n);
#define lua_pushlstring interrupted_copy
#include "../../native/values.c"
#undef lua_pushlstring

static int mode, live, fail_growth;
static _Thread_local void *context;
void *ws_copy_context(void) { return context; }
void *ws_set_copy_context(void *p) {
    void *previous = context;
    context = p;
    return previous;
}
typedef void *(*Allocator)(void *, void *, size_t, size_t);
static lua_State *(*newstate)(void);
static Allocator (*getallocf)(lua_State *, void **);
static void (*setallocf)(lua_State *, Allocator, void *);
static void (*close_state)(lua_State *);

struct Allocation {
    Allocator original;
    void *ud;
};

static ws_str owned(const char *text, size_t len) {
    char *p = malloc(len);
    assert(p);
    memset(p, *text, len);
    live++;
    return (ws_str){p, len};
}

void ws_free(ws_str s) {
    if (s.ptr) { free((void *)s.ptr); live--; }
}

ws_str ws_error(void) { return owned("e", 1); }

static void *allocate(void *ud, void *p, size_t old, size_t size) {
    struct Allocation *allocation = ud;
    if (fail_growth && size && (!p || size > old)) return NULL;
    return allocation->original(allocation->ud, p, old, size);
}

static void interrupted_copy(lua_State *L, const char *p, size_t n) {
    int interrupted = mode;
    mode = 0;
    if (interrupted == 1) {
        ws_str nested;
        prepare_str(L, &nested);
        nested = owned("n", 128);
        push_str(L, &nested);
        size_t len;
        assert(*luaL_checklstring(L, -1, &len) == 'n' && len == 128);
        lua_settop(L, -2);
        assert(live == 1); /* the outer string is still owned */
    } else if (interrupted == 2) {
        lua_pushstring(L, "copy interrupted");
        lua_error(L);
    }
    lua_pushlstring(L, p, n);
}

static int convert(lua_State *L) {
    ws_str s;
    prepare_str(L, &s);
    s = owned("x", 256 * 1024);
    if (mode == 3) fail_growth = 1;
    push_str(L, &s);
    return 1;
}

static void test(int interrupted) {
    /* Non-GC64 LuaJIT requires its own allocator and luaL_newstate.
     * Wrap that allocator instead of replacing it with malloc/realloc. */
    lua_State *L = newstate();
    assert(L);
    struct Allocation allocation;
    allocation.original = getallocf(L, &allocation.ud);
    assert(allocation.original);
    setallocf(L, allocate, &allocation);
    lua_pushcclosure(L, convert, 0);
    mode = interrupted;
    int rc = ws_pcall(L, 0, 1);
    fail_growth = 0;
    assert(live == 0); /* including when Lua longjmps out of copy_str */
    if (interrupted >= 2) assert(rc != 0);
    else {
        assert(rc == 0);
        size_t len;
        const char *p = luaL_checklstring(L, -1, &len);
        assert(len == 256 * 1024);
        for (size_t i = 0; i < len; i++) assert(p[i] == 'x');
    }
    lua_settop(L, 0);
    /* An interrupted call must also restore the conversion context. */
    lua_pushcclosure(L, convert, 0);
    mode = 0;
    assert(ws_pcall(L, 0, 1) == 0 && live == 0);
    lua_settop(L, 0);
    lua_pushcclosure(L, copy_str, 0);
    assert(ws_pcall(L, 0, 1) != 0); /* escaped private function is safe */
    setallocf(L, allocation.original, allocation.ud);
    close_state(L);
}

#ifdef _WIN32
__declspec(dllexport)
#endif
int luaopen_ws_strings(lua_State *L) {
#ifdef _WIN32
    HMODULE runtime;
    DWORD flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    assert(GetModuleHandleExA(flags, (LPCSTR)__builtin_return_address(0), &runtime));
#define SYMBOL(name) ((void (*)(void))GetProcAddress(runtime, name))
#else
    void *runtime = NULL;
#define SYMBOL(name) ((void (*)(void))dlsym(RTLD_DEFAULT, name))
#endif
    assert(ws_bind_api(runtime));
    newstate = (lua_State *(*)(void))SYMBOL("luaL_newstate");
    getallocf = (Allocator (*)(lua_State *, void **))SYMBOL("lua_getallocf");
    setallocf = (void (*)(lua_State *, Allocator, void *))SYMBOL("lua_setallocf");
    close_state = (void (*)(lua_State *))SYMBOL("lua_close");
    assert(newstate && getallocf && setallocf && close_state);
    for (int i = 0; i < 4; i++) test(i);
    lua_pushboolean(L, 1);
    return 1;
}
