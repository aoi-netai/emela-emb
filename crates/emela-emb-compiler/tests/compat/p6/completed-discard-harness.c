
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_children(void);
int test_heap_empty(void);
void test_cancel(EmelaFiber);
static EmelaTask task;
static int started;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t s) { (void)s; }
void board_wait_start(EmelaTask t, int32_t n) { assert(n == ++started); task = t; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    int32_t result = -1; started = 0;
    EmelaFiber f = legacy_abandoned_start(42, &result);
    while (emela_scheduler_run(1)) {}
    assert(started == 1 && !emela_fiber_done(f) && test_children() == 0);
    assert(emela_task_complete_int(task, 0));
    while (emela_scheduler_run(1)) {}
    assert(started == 2 && !emela_fiber_done(f) && test_children() == 0);
    if (i % 2) test_cancel(f);
    else assert(emela_task_complete_int(task, 0));
    while (!emela_fiber_done(f)) {
      uint32_t steps = emela_scheduler_run(1);
      assert(steps || emela_fiber_done(f));
    }
    assert(result == (i % 2 ? -1 : 42));
    assert(!emela_task_complete_int(task, 0));
    emela_fiber_release(f);
    assert(test_heap_empty());
  }
}
