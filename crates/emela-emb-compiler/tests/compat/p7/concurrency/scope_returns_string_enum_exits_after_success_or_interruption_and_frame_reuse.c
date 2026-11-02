
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
    int cancel_child = i % 2, cancel_parent = i % 5 == 0, discard = i % 3 == 0;
    count = 0; emela_type_{ty} result = {0};
    EmelaFiber f = legacy_terminal_start(42, cancel_child, discard ? NULL : &result);
    drain(); assert(count == 2 && !emela_fiber_done(f));
    if (cancel_parent) test_cancel(f);
    else {
      if (!cancel_child && i % 4) { assert(emela_task_complete_int(tasks[3], 30)); drain(); }
      assert(emela_task_complete_int(tasks[4], 40)); drain();
      if (!cancel_child && !emela_fiber_done(f)) assert(emela_task_complete_int(tasks[3], 30));
    }
    drain(); assert(emela_fiber_done(f)); emela_fiber_release(f);
    if (!discard && !cancel_parent) {
      if (cancel_child) assert(result.tag == 1 && result.payload.v1.f0);
      else {
        assert(result.tag == 0 && result.payload.v0.f0.tag == 0);
        EmelaString text = result.payload.v0.f0.payload.v0.f0.f0;
        assert(text.owner && text.length == 2 && memcmp(text.data, "42", 2) == 0);
      }
      {release}(&result);
    }
    assert(test_heap_empty());
  }
}

