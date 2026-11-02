
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask tasks[2];
static int count, order, mode;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
void board_wait_start(EmelaTask t, int32_t n) { (void)n; assert(count < 2); tasks[count++] = t; }
void mark(int32_t n) {
  assert(n == ++order);
  if (n == 3 && mode) assert(emela_task_complete_int(tasks[1], 99));
}
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int32_t value = -1;
    count = order = 0; mode = i % 2;
    EmelaFiber f = legacy_work_start(40, &value);
    while (count != 2) assert(emela_scheduler_run(1));
    if (!mode) {
      assert(emela_task_complete_int(tasks[1], 99));
      while (emela_scheduler_run(1)) {}
      assert(!emela_fiber_done(f) && value == -1);
    }
    assert(emela_task_complete_int(tasks[0], 42));
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(value == 42 && order == 3);
    assert(!emela_task_complete_int(tasks[1], 99));
    emela_fiber_release(f);
    while (emela_scheduler_run(1)) {}
  }
}
