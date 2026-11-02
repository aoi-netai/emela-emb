#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    emela_type_{ty} value = {0};
    EmelaFiber f = legacy_terminal_start(i, i % 3 ? &value : NULL);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    emela_fiber_release(f);
    if (i % 3) { assert(value.tag == 0 && value.payload.v0.f0 == i); {release}(&value); }
    assert(test_heap_empty());
  }
}
