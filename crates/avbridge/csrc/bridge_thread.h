/* Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version. */

/* The few threading primitives the bridge needs (one worker thread fed by a queue), over Win32 or
   POSIX threads. Not a general threading layer: no error returns beyond thread creation. */

#ifndef BRIDGE_THREAD_H
#define BRIDGE_THREAD_H

#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

typedef CRITICAL_SECTION BridgeMutex;
typedef CONDITION_VARIABLE BridgeCond;
typedef struct {
    HANDLE handle;
    void (*fn)(void *);
    void *arg;
} BridgeThread;

static inline void bridge_mutex_init(BridgeMutex *m) {
    InitializeCriticalSection(m);
}
static inline void bridge_mutex_destroy(BridgeMutex *m) {
    DeleteCriticalSection(m);
}
static inline void bridge_mutex_lock(BridgeMutex *m) {
    EnterCriticalSection(m);
}
static inline void bridge_mutex_unlock(BridgeMutex *m) {
    LeaveCriticalSection(m);
}
static inline void bridge_cond_init(BridgeCond *c) {
    InitializeConditionVariable(c);
}
static inline void bridge_cond_destroy(BridgeCond *c) {
    (void)c;
}
static inline void bridge_cond_wait(BridgeCond *c, BridgeMutex *m) {
    SleepConditionVariableCS(c, m, INFINITE);
}
static inline void bridge_cond_broadcast(BridgeCond *c) {
    WakeAllConditionVariable(c);
}

static DWORD WINAPI bridge_thread_entry(LPVOID param) {
    BridgeThread *thread = (BridgeThread *)param;
    thread->fn(thread->arg);
    return 0;
}

/* Starts `fn(arg)` on a new thread. Returns 0 on success. `thread` must outlive the thread. */
static inline int bridge_thread_start(BridgeThread *thread, void (*fn)(void *), void *arg) {
    thread->fn = fn;
    thread->arg = arg;
    thread->handle = CreateThread(NULL, 0, bridge_thread_entry, thread, 0, NULL);
    return thread->handle ? 0 : -1;
}

static inline void bridge_thread_join(BridgeThread *thread) {
    WaitForSingleObject(thread->handle, INFINITE);
    CloseHandle(thread->handle);
}
#else
#include <pthread.h>

typedef pthread_mutex_t BridgeMutex;
typedef pthread_cond_t BridgeCond;
typedef struct {
    pthread_t handle;
    void (*fn)(void *);
    void *arg;
} BridgeThread;

static inline void bridge_mutex_init(BridgeMutex *m) {
    pthread_mutex_init(m, NULL);
}
static inline void bridge_mutex_destroy(BridgeMutex *m) {
    pthread_mutex_destroy(m);
}
static inline void bridge_mutex_lock(BridgeMutex *m) {
    pthread_mutex_lock(m);
}
static inline void bridge_mutex_unlock(BridgeMutex *m) {
    pthread_mutex_unlock(m);
}
static inline void bridge_cond_init(BridgeCond *c) {
    pthread_cond_init(c, NULL);
}
static inline void bridge_cond_destroy(BridgeCond *c) {
    pthread_cond_destroy(c);
}
static inline void bridge_cond_wait(BridgeCond *c, BridgeMutex *m) {
    pthread_cond_wait(c, m);
}
static inline void bridge_cond_broadcast(BridgeCond *c) {
    pthread_cond_broadcast(c);
}

static void *bridge_thread_entry(void *param) {
    BridgeThread *thread = (BridgeThread *)param;
    thread->fn(thread->arg);
    return NULL;
}

/* Starts `fn(arg)` on a new thread. Returns 0 on success. `thread` must outlive the thread. */
static inline int bridge_thread_start(BridgeThread *thread, void (*fn)(void *), void *arg) {
    thread->fn = fn;
    thread->arg = arg;
    return pthread_create(&thread->handle, NULL, bridge_thread_entry, thread) == 0 ? 0 : -1;
}

static inline void bridge_thread_join(BridgeThread *thread) {
    pthread_join(thread->handle, NULL);
}
#endif

#endif
