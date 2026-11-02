
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int marks;
void mark(int32_t n) { assert(n == 1); marks++; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int32_t result = 0;
    EmelaFiber f = legacy_skip_start(&result);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == 7 && marks == i + 1);
    emela_fiber_release(f);
  }
}
