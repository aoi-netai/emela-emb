
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
    int bad = i % 2, cancel = i % 5 == 0, discard = i % 3 == 0;
    count = 0; EmelaString result = {0};
    EmelaFiber f = legacy_checked_start(42, bad, discard ? NULL : &result);
    drain(); assert(count == 2 && !emela_fiber_done(f));
    if (cancel) test_cancel(f);
    else {
      assert(emela_task_complete_int(tasks[2], 20)); drain();
      if (!bad) { assert(!emela_fiber_done(f)); assert(emela_task_complete_int(tasks[1], 10)); }
    }
    drain(); assert(emela_fiber_done(f)); emela_fiber_release(f);
    if (!cancel && !discard) {
      assert(result.owner && result.length == (bad ? 2 : 4));
      assert(memcmp(result.data, bad ? "42" : "4343", result.length) == 0);
      emela_embedded_release_string(&result);
    }
    assert(test_heap_empty());
  }
}

