
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask pending;
static int called;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t, int32_t n) { assert(n == 1 && !called++); pending = t; }
void mark(int32_t n) { (void)n; abort(); }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
int test_heap_empty(void);
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int32_t result = -1; called = 0;
    EmelaFiber f = legacy_cancel_start(i % 2, &result);
    while (emela_scheduler_run(1)) {}
    assert(called == 1 && !emela_fiber_done(f));
    assert(emela_task_complete_int(pending, 1));
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == 42 && !emela_task_complete_int(pending, 0));
    emela_fiber_release(f); assert(test_heap_empty());
  }
}
