
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask pending;
static unsigned count;
static const int order[] = {11, 12, 13, 21, 14, 22, 23};
void mark(int32_t n) { assert(count < sizeof(order) / sizeof(order[0]) && n == order[count++]); }
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
void board_wait_start(EmelaTask t, int32_t n) { (void)n; pending = t; }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    count = 0;
    int32_t result = -1;
    EmelaFiber f = legacy_work_start(&result);
    while (emela_scheduler_run(1)) assert(result == -1);
    assert(count == 1);
    assert(emela_task_complete_int(pending, 1));
    while (emela_scheduler_run(1)) assert(result == -1);
    assert(count == 5);
    assert(emela_task_complete_int(pending, 2));
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == 3 && count == 7);
    emela_fiber_release(f); assert(test_heap_empty());
  }
  return 0;
}
