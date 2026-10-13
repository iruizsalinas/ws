/* Runtime binding, once per loaded module. Lua 5.2 has a different
 * continuation ABI from 5.3+, so each gets its exact function type. */
#ifndef _WIN32
#define _GNU_SOURCE
#include <dlfcn.h>
#include <pthread.h>
#endif
#include "luaapi.h"

static int (*pcall51)(lua_State *, int, int, int);
static int (*pcall52)(lua_State *, int, int, int, int, lua_CFunction);
typedef int (*Continuation)(lua_State *, int, intptr_t);
static int (*pcall53)(lua_State *, int, int, int, intptr_t, Continuation);

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#define X(ret, name, args) ret (*name) args;
WS_LUA_API(X)
#undef X
static INIT_ONCE once = INIT_ONCE_STATIC_INIT;
#define SYMBOL(name) ((void (*)(void))GetProcAddress(runtime, name))
#else
static pthread_once_t once = PTHREAD_ONCE_INIT;
static int bound;
#define SYMBOL(name) ((void (*)(void))dlsym(RTLD_DEFAULT, name))
#endif

static int bind(void *runtime) {
#ifdef _WIN32
#define X(ret, name, args) \
    if (!(name = (ret (*) args)SYMBOL(#name))) return 0;
    WS_LUA_API(X)
#undef X
#else
    (void)runtime;
#endif
    pcall51 = (int (*)(lua_State *, int, int, int))SYMBOL("lua_pcall");
    if (!pcall51) {
        if (SYMBOL("lua_getctx"))
            pcall52 = (int (*)(lua_State *, int, int, int, int, lua_CFunction))SYMBOL("lua_pcallk");
        else
            pcall53 = (int (*)(lua_State *, int, int, int, intptr_t, Continuation))SYMBOL("lua_pcallk");
        if (!pcall52 && !pcall53) return 0;
    }
    /* Workers can outlive lua_close; keep their code mapped. Retain only
     * one loader reference, even when many Lua states require the module. */
#ifdef _WIN32
    HMODULE self;
    DWORD flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN;
    return GetModuleHandleExA(flags, (LPCSTR)&once, &self) != 0;
#else
    Dl_info info;
    return dladdr((const void *)&once, &info) && info.dli_fname &&
           dlopen(info.dli_fname, RTLD_NOW | RTLD_NOLOAD | RTLD_NODELETE) != NULL;
#endif
}

#ifdef _WIN32
static BOOL CALLBACK initialize(PINIT_ONCE o, PVOID runtime, PVOID *bound) {
    (void)o;
    if (!bind(runtime)) return FALSE;
    *bound = runtime;
    return TRUE;
}
#else
static void initialize(void) { bound = bind(NULL); }
#endif

int ws_bind_api(void *runtime) {
#ifdef _WIN32
    PVOID bound;
    return InitOnceExecuteOnce(&once, initialize, runtime, &bound) && bound == runtime;
#else
    (void)runtime;
    return pthread_once(&once, initialize) == 0 && bound;
#endif
}

int ws_pcall(lua_State *L, int nargs, int nresults) {
    if (pcall51) return pcall51(L, nargs, nresults, 0);
    if (pcall52) return pcall52(L, nargs, nresults, 0, 0, NULL);
    return pcall53(L, nargs, nresults, 0, 0, NULL);
}
