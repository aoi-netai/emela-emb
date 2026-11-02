#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int order;
void mark(int32_t n) { assert(n == ++order); }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
int test_heap_empty(void);
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int32_t result = -1; order = 0;
    EmelaFiber f = legacy_work_start(i, &result);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == 2 * (i + 1) && order == 4); emela_fiber_release(f);
    order = 0; f = legacy_unit_start();
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(order == 2); emela_fiber_release(f);
    assert(test_heap_empty());
  }
}
