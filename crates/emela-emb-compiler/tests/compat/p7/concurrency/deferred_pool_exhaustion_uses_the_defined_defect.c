#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)m; (void)n; assert(k == EMELA_DEFECT_HEAP_EXHAUSTED); exit(0); }
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
  EmelaFiber f = legacy_full_start(NULL);
  while (!emela_fiber_done(f)) emela_scheduler_run(1);
  abort();
}
