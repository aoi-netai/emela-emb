
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask tasks[2];
static int count, marks;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
void board_wait_start(EmelaTask t, int32_t n) { assert(n == 40 + count); tasks[count++] = t; }
void mark(int32_t n) { assert(n == 2); marks++; }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    EmelaString result = {0}; count = marks = 0;
    EmelaFiber f = legacy_managed_start(40, i % 3 ? &result : NULL);
    while (count < 2) assert(emela_scheduler_run(1));
    if (i % 2) {
      assert(emela_task_complete_int(tasks[1], 41));
      while (emela_scheduler_run(1)) {}
      assert(!emela_fiber_done(f));
    }
    assert(emela_task_complete_int(tasks[0], 40));
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    assert(marks == 1);
    emela_fiber_release(f);
    if (i % 3) { assert(result.owner && result.length == 2 && memcmp(result.data, "40", 2) == 0); emela_embedded_release_string(&result); }
    assert(!emela_task_complete_int(tasks[1], 0));
  }
}
