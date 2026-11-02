#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask tasks[3];
static int count, order, queue_old;
void mark(int32_t n) { assert(n == ++order); if (n == 2 && queue_old) assert(emela_task_complete_int(tasks[0], 99)); }
void board_wait_start(EmelaTask t, int32_t n) { assert(n >= 1 && n <= 3); tasks[n - 1] = t; count++; }
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t n) { (void)n; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
int test_heap_empty(void);
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int32_t value = -1; count = order = 0; queue_old = i % 2;
    EmelaFiber f = legacy_work_start(&value);
    while (emela_scheduler_run(1)) {}
    assert(count == 2 && order == 1);
    assert(emela_task_complete_int(tasks[1], 42));
    while (emela_scheduler_run(1)) {}
    assert(count == 3 && order == 3 && !emela_fiber_done(f));
    assert(!emela_task_complete_int(tasks[0], 0));
    assert(emela_task_complete_int(tasks[2], 3));
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(value == 42 && order == 5); emela_fiber_release(f);
    assert(test_heap_empty());
  }
}
