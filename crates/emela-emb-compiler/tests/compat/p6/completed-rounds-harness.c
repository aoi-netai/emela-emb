
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_children(void);
int test_heap_empty(void);
void test_weak_child_links(void);
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t s) { (void)s; }
void board_wait_start(EmelaTask t, int32_t n) { (void)t; (void)n; abort(); }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    test_weak_child_links(); assert(test_heap_empty());
    int32_t result = -1;
    EmelaFiber f = legacy_work_start(1000, i % 2 ? &result : NULL);
    while (!emela_fiber_done(f)) {
      assert(emela_scheduler_run(1));
      assert(test_children() <= 2);
    }
    if (i % 2) assert(result == 3000);
    emela_fiber_release(f);
    assert(test_children() == 0 && test_heap_empty());
  }
}
