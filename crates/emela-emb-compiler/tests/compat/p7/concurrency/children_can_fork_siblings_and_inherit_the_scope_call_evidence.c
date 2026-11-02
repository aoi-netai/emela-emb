
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask task;
static int marks;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
void board_wait_start(EmelaTask t, int32_t n) { assert(n == 1); task = t; }
void mark(int32_t n) { assert(n == 10 + marks++); }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    int32_t result = 0; marks = 0;
    EmelaFiber f = legacy_siblings_start(&result);
    while (emela_scheduler_run(1)) {}
    assert(marks == 2 && !emela_fiber_done(f));
    assert(emela_task_complete_int(task, 42));
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(result == 42);
    emela_fiber_release(f);
  }
}
