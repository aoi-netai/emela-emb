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
  void *old = NULL;
  EmelaFiber f = legacy_failure_start(&old);
  while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
  emela_fiber_release(f);
  void *same = legacy_keep(old); assert(same == old); emela_embedded_release(same);
  for (int i = 0; i < 1000; i++) {
    void *next = NULL; uint32_t kind;
    f = legacy_failure_start(i % 2 ? &next : NULL);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    emela_fiber_release(f);
    memcpy(&kind, old, sizeof(kind)); assert(kind == 2);
    if (i % 2) { assert(next != old); memcpy(&kind, next, sizeof(kind)); assert(kind == 2); emela_embedded_release(next); }
  }
  emela_embedded_release(old);
}
