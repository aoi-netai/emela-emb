
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask pending[2];
static int32_t inputs[2];
static unsigned count;
static int immediate;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
void board_wait_start(EmelaTask task, int32_t n) {
  if (immediate) { assert(emela_task_complete_int(task, n)); return; }
  assert(count < 2); pending[count] = task; inputs[count++] = n;
}
int main(void) {
  int32_t a = -1, b = -1;
  EmelaFiber x = legacy_work_start(40, &a);
  EmelaFiber y = legacy_work_start(10, &b);
  while (emela_scheduler_run(1)) assert(a == -1 && b == -1);
  assert(count == 2);
  assert(emela_task_complete_int(pending[1], inputs[1]));
  while (!emela_fiber_done(y)) assert(emela_scheduler_run(1));
  assert(a == -1 && b == 11); emela_fiber_release(y);
  assert(emela_task_complete_int(pending[0], inputs[0]));
  while (!emela_fiber_done(x)) assert(emela_scheduler_run(1));
  assert(a == 41); emela_fiber_release(x); assert(test_heap_empty());
  immediate = 1;
  for (int i = 0; i < 1000; i++) {
    int32_t result = -1;
    EmelaFiber f = legacy_work_start(i, &result);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == i + 1); emela_fiber_release(f); assert(test_heap_empty());
  }
  return 0;
}
