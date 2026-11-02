
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_cancel(EmelaFiber f);
static EmelaTask tasks[7];
static unsigned count;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k,n,m); abort(); }
void board_wait_start(EmelaTask t, int32_t n) { assert(n >= 10 && n <= 60 && n % 10 == 0); tasks[n / 10] = t; count++; }
static void drain(void) { while (emela_scheduler_run(1)) {} }

int main(void) {
  for (int i = 0; i < 1000; ++i) {
    int cancel = i % 5, discard = i % 3 == 0;
    count = 0; emela_type_{ty} result = {0};
    EmelaFiber f = legacy_echoed_start(42, discard ? NULL : &result);
    drain(); assert(count == 1 && !emela_fiber_done(f));
    if (cancel == 1) test_cancel(f);
    else {
      assert(emela_task_complete_int(tasks[5], 50)); drain();
      assert(count == 2 && !emela_fiber_done(f));
      if (cancel == 2) test_cancel(f);
      else assert(emela_task_complete_int(tasks[6], 60));
    }
    drain(); assert(emela_fiber_done(f)); emela_fiber_release(f);
    if (cancel != 1 && cancel != 2 && !discard) {
      assert(result.tag == 0 && result.payload.v0.f0.f0.owner);
      assert(result.payload.v0.f0.f0.length == 2 && memcmp(result.payload.v0.f0.f0.data, "42", 2) == 0);
      assert(result.payload.v0.f0.f1.tag == 0 && result.payload.v0.f0.f1.payload.v0.f0.owner && result.payload.v0.f0.f1.payload.v0.f0.length == 2 && memcmp(result.payload.v0.f0.f1.payload.v0.f0.data, "42", 2) == 0);
      {release}(&result);
    }
    assert(test_heap_empty());
  }
}

