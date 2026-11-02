
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k,n,m); abort(); }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    int cancel = i % 2, discard = i % 3 == 0;
    void *list = NULL;
    EmelaFiber f = legacy_work_start(42, cancel, discard ? NULL : &list);
    while (!emela_fiber_done(f)) assert(emela_scheduler_run(1));
    emela_fiber_release(f);
    if (!discard && !cancel) {
      emela_type_{ty} polled = legacy_head(list);
      void *rest = legacy_tail(list);
      emela_type_{ty} awaited = legacy_head(rest);
      void *last = legacy_tail(rest);
      emela_type_{ty} captured = legacy_head(last);
      emela_embedded_release(last); emela_embedded_release(rest); emela_embedded_release(list);
      assert(polled.f0.owner && polled.f0.owner == awaited.f0.owner);
      assert(polled.f1.tag == 0 && polled.f1.payload.v0.f0.owner == awaited.f1.payload.v0.f0.owner);
      assert(polled.f0.length == 2 && memcmp(polled.f0.data, "42", 2) == 0);
      assert(polled.f1.payload.v0.f0.length == 2 && memcmp(polled.f1.payload.v0.f0.data, "42", 2) == 0);
      {release}(&polled);
      assert(awaited.f0.length == 2 && memcmp(awaited.f0.data, "42", 2) == 0);
      assert(captured.f0.length == 2 && memcmp(captured.f0.data, "52", 2) == 0);
      assert(captured.f1.tag == 0 && captured.f1.payload.v0.f0.length == 2 && memcmp(captured.f1.payload.v0.f0.data, "53", 2) == 0);
      {release}(&awaited); {release}(&captured);
    } else assert(list == NULL);
    assert(test_heap_empty());
  }
}
