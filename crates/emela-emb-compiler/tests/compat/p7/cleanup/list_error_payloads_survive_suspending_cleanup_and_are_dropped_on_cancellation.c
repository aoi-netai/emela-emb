#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_cancel(EmelaFiber h);
static EmelaTask task;
static int waits;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t, int32_t n) { assert(n == ++waits); task = t; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k,n,m); abort(); }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    int32_t result = -1; waits = 0;
    EmelaFiber f = legacy_caught_start(INT32_MIN, (i / 2) % 2, &result);
    while (emela_scheduler_run(1)) {} assert(waits == 1);
    assert(emela_task_complete_int(task, 0));
    while (emela_scheduler_run(1)) {} assert(waits == 2 && result == -1);
    assert(emela_task_complete_int(task, 0));
    while (emela_scheduler_run(1)) {} assert(waits == 3 && result == -1);
    if (i % 2) test_cancel(f);
    else assert(emela_task_complete_int(task, 0));
    while (!emela_fiber_done(f)) emela_scheduler_run(1);
    emela_fiber_release(f);
    assert(result == (i % 2 ? -1 : 12));
    assert(!emela_task_complete_int(task, 0));
    assert(test_heap_empty());
  }
}
