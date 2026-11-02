#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int expected;
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { assert(expected && k == EMELA_DEFECT_PANIC && n == 41); assert(memcmp(m, "deferred was polled after its scope ended", n) == 0); exit(0); }
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
  emela_function_type_{ty} old = {0};
  EmelaFiber f = legacy_leak_start(&old);
  while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
  emela_fiber_release(f);
  for (int i = 0; i < 1000; i++) {
    emela_function_type_{ty} next = {0};
    f = legacy_leak_start(&next);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    emela_fiber_release(f); emela_embedded_release_function_{ty}(&next);
  }
  expected = 1; (void)old.call(old.env); abort();
}
