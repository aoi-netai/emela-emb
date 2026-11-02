
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask tasks[2];
static int count, order;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "defect %d: %.*s\n", k, n, m); abort(); }
void board_wait_start(EmelaTask t, int32_t n) { assert(count < 2); assert(n == 40 + count); tasks[count++] = t; }
void mark(int32_t n) { assert(n == ++order); }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int32_t value = -1;
    count = order = 0;
    EmelaFiber f = legacy_work_start(40, &value);
    assert(emela_scheduler_run(0) == 0 && count == 0 && order == 0);
    while (count != 2) assert(emela_scheduler_run(1));
    assert(order == 2 && value == -1);
    assert(emela_task_complete_int(tasks[0], 42));
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(order == 3 && value == 42);
    assert(!emela_task_complete_int(tasks[1], 99));
    emela_fiber_release(f);
  }
  return 0;
}

