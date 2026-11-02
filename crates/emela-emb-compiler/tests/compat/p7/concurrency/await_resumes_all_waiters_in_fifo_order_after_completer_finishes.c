#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int count;
void mark(int32_t n) { static const int expected[] = {0,30,31,11,12,40}; assert(count < 6 && n == expected[count++]); }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
int test_heap_empty(void);
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int32_t result = -1; count = 0;
    EmelaFiber f = legacy_work_start(&result);
    assert(emela_scheduler_run(0) == 0 && count == 0 && result == -1);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == 10 && count == 6); emela_fiber_release(f);
    assert(test_heap_empty());
  }
}
