#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask task;
int test_heap_empty(void);
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t, int32_t n) { (void)n; task = t; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k,n,m); abort(); }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    int32_t result = -1;
    EmelaFiber f = legacy_caught_start(i % 2, &result);
    while (emela_scheduler_run(1)) {}
    assert(emela_task_complete_int(task, INT32_MIN));
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    emela_fiber_release(f);
    assert(result == (i % 2 ? 11 : INT32_MIN));
    assert(test_heap_empty());
  }
}
