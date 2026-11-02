#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
int test_heap_empty(void);
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int32_t result = 0;
    EmelaFiber f = legacy_pair_start(i, &result);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == 2 * i + 1); emela_fiber_release(f);
    f = legacy_flag_start(i % 2, &result);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == i % 2); emela_fiber_release(f);
    assert(test_heap_empty());
  }
}
