#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  assert(k == EMELA_DEFECT_PANIC && n == 21 && memcmp(m, "fiber was interrupted", n) == 0); exit(0);
}
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
  int32_t value = -1; EmelaFiber f = legacy_work_start(&value);
  while (!emela_fiber_done(f)) emela_scheduler_run(1);
  abort();
}
