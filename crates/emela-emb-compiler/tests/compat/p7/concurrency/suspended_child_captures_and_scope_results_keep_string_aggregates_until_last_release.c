
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask tasks[2];
static int count;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t, int32_t n) { assert(n >= 1 && n <= 2); tasks[n-1] = t; count++; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k,n,m); abort(); }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    emela_type_{ty} result = {0}; count = 0;
    int cancel = i % 2, discard = i % 3 == 0;
    EmelaFiber f = legacy_scoped_start(42, cancel, discard ? NULL : &result);
    while (emela_scheduler_run(1)) {} assert(count == 2);
    assert(emela_task_complete_int(tasks[1], 0));
    while (emela_scheduler_run(1)) {}
    if (!cancel) { assert(!emela_fiber_done(f)); assert(emela_task_complete_int(tasks[0], 0)); }
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    emela_fiber_release(f);
    assert(!emela_task_complete_int(tasks[0], 0));
    if (!discard) {
      assert(result.f0.owner && result.f0.length == 4);
      assert(memcmp(result.f0.data, cancel ? "none" : "4244", 4) == 0);
      if (cancel) assert(result.f1.tag == 1);
      else assert(result.f1.tag == 0 && result.f1.payload.v0.f0.owner && result.f1.payload.v0.f0.length == 2 && memcmp(result.f1.payload.v0.f0.data, "43", 2) == 0);
      {release}(&result);
    }
    assert(test_heap_empty());
  }
}
