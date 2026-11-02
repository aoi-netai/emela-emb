#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
int test_heap_empty(void);
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int64_t wide = 0; double real = 0, input = i % 2 ? -0.0 : -1.2345678901234567;
    int32_t flag = -1;
    EmelaFiber f = legacy_wide_start(INT64_MIN, &wide);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(wide == INT64_MIN); emela_fiber_release(f);
    f = legacy_real_start(input, &real);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(memcmp(&input, &real, sizeof(double)) == 0); emela_fiber_release(f);
    f = legacy_flag_start(i % 2, &flag);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(flag == i % 2); emela_fiber_release(f);
    assert(test_heap_empty());
  }
}
