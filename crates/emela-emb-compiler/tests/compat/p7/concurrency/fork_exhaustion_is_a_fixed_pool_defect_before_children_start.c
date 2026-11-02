
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
void mark(int32_t n) { (void)n; abort(); }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)m; (void)n; assert(k == EMELA_DEFECT_HEAP_EXHAUSTED); exit(0); }
int main(void) {
  EmelaFiber f = legacy_full_start(NULL);
  while (!emela_fiber_done(f)) emela_scheduler_run(1);
  abort();
}
