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
    EmelaString text = {0}; void *list = NULL;
    EmelaFiber f = legacy_text_start(-2147483647 - 1, i % 2 ? &text : NULL);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    emela_fiber_release(f);
    f = legacy_list_start(i, i % 2 ? &list : NULL);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    emela_fiber_release(f);
    if (i % 2) {
      assert(text.owner && text.length == 11 && memcmp(text.data, "-2147483648", 11) == 0);
      assert(legacy_sum(list) == 2 * i + 1);
      emela_embedded_release_string(&text); emela_embedded_release(list);
    }
    assert(test_heap_empty());
  }
}
