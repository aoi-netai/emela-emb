
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_cancel(EmelaFiber h);
static EmelaTask tasks[4];
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t, int32_t n) { assert(n == 10 || n == 20 || n == 30); tasks[n / 10] = t; }
static void drain(void) { while (emela_scheduler_run(1)) {} }
int main(void) {
  for (unsigned i = 0; i < 1000; i++) {
    assert(legacy_escaped_result(40) == 83);
    assert(test_heap_empty());
    char composite_buffer[128];
    EmelaString composite = legacy_composite(42, composite_buffer, sizeof(composite_buffer));
    assert(composite.length == 4 && memcmp(composite.data,"4242",4)==0);
    emela_embedded_release(composite.owner); assert(test_heap_empty());
    EmelaString result = {0};
    EmelaFiber fiber = legacy_shared_start(42, &result); drain();
    assert(emela_task_complete_int(tasks[1], 0)); drain();
    assert(emela_task_complete_int(tasks[2], 0)); drain();
    assert(result.length == 31 && memcmp(result.data, "42 child parent/42 child parent", 31) == 0);
    emela_fiber_release(fiber); emela_embedded_release(result.owner); assert(test_heap_empty());
    fiber = legacy_shared_start(42, NULL); drain();
    assert(emela_task_complete_int(tasks[2], 0)); drain();
    assert(emela_task_complete_int(tasks[1], 0)); drain();
    emela_fiber_release(fiber); assert(test_heap_empty());
    for (int bad = 0; bad < 2; bad++) {
      fiber = legacy_failed_start(42, bad, &result); drain();
      assert(emela_task_complete_int(tasks[3], 0)); drain();
      assert(result.length == 10 && memcmp(result.data,"42 failure",10)==0);
      emela_fiber_release(fiber); emela_embedded_release(result.owner); assert(test_heap_empty());
    }
    fiber = legacy_failed_start(42, 1, NULL); drain();
    test_cancel(fiber); drain(); emela_fiber_release(fiber); assert(test_heap_empty());
    for (unsigned step = 0; step < 2; step++) {
      fiber = legacy_shared_start(42, &result); drain();
      if (step) { assert(emela_task_complete_int(tasks[1], 0)); drain(); }
      test_cancel(fiber); drain(); emela_fiber_release(fiber); assert(test_heap_empty());
    }
  }
  return 0;
}
