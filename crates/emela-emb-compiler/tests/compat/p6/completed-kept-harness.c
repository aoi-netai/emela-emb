
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_children(void);
int test_heap_empty(void);
static EmelaTask task;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t s) { (void)s; }
void board_wait_start(EmelaTask t, int32_t n) { assert(n == 1); task = t; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    void *result = NULL;
    EmelaFiber f = legacy_kept_start(42, i % 3 ? &result : NULL);
    while (emela_scheduler_run(1)) {}
    assert(!emela_fiber_done(f) && test_children() == 1);
    for (int n = 0; n < 10; ++n) {
      EmelaFiber g = legacy_work_start(1, NULL);
      while (!emela_fiber_done(g)) assert(emela_scheduler_run(1));
      emela_fiber_release(g);
      assert(test_children() == 1);
    }
    assert(emela_task_complete_int(task, 0));
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    emela_fiber_release(f);
    assert(test_children() == 0);
    if (i % 3) {
      for (int n = 0; n < 3; ++n) {
        EmelaString text = legacy_head(result);
        assert(text.owner && text.length == 2 && memcmp(text.data, "42", 2) == 0);
        emela_embedded_release_string(&text);
        void *next = legacy_tail(result);
        emela_embedded_release(result); result = next;
      }
      assert(result == NULL);
    }
    assert(test_heap_empty());
  }
}
