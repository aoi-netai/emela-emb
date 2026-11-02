
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask pending;
static int queued;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
void board_wait_start(EmelaTask t, int32_t n) { (void)n; pending = t; }
void notify(void) { if (queued) assert(emela_task_complete_int(pending, 0)); }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    queued = i % 2;
    int32_t result = -1;
    EmelaFiber f = legacy_work_start(i, &result);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == i && !emela_task_complete_int(pending, 999));
    emela_fiber_release(f); assert(test_heap_empty());
  }
  while (emela_scheduler_run(1)) {}
  return 0;
}
