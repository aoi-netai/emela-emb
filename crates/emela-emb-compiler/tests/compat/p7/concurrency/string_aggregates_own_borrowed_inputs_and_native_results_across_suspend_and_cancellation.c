
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_cancel(EmelaFiber h);
static EmelaTask task;
static int waits;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t, int32_t n) { assert(n == ++waits); task = t; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k,n,m); abort(); }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    emela_type_{ty} native = legacy_make(INT32_MIN);
    assert(native.f0.owner && native.f1.payload.v0.f0.owner);
    assert(native.f0.length == 11 && memcmp(native.f0.data, "-2147483648", 11) == 0);
    {release}(&native);
    assert(test_heap_empty());
    char left[] = {'a', 0, 'b'}, right[] = "xyz";
    emela_type_{ty} input = {.f0 = {.data = left, .length = 3},
      .f1 = {.tag = 0, .payload.v0.f0 = {.data = right, .length = 3}}};
    emela_type_{ty} result = {0}; waits = 0;
    int mode = i % 6;
    if (mode == 5) input = legacy_make(42);
    void *owner0 = input.f0.owner, *owner1 = input.f1.payload.v0.f0.owner;
    EmelaFiber f = legacy_delayed_start(input, mode != 1, mode == 4 ? NULL : &result);
    if (mode == 5) {release}(&input);
    memset(left, 'x', 3); memset(right, 'x', 3);
    while (emela_scheduler_run(1)) {} assert(waits == 1);
    if (mode == 2) test_cancel(f);
    else {
      assert(emela_task_complete_int(task, 0));
      while (emela_scheduler_run(1)) {} assert(waits == 2);
      if (mode == 3) test_cancel(f);
      else assert(emela_task_complete_int(task, 0));
    }
    while (!emela_fiber_done(f)) emela_scheduler_run(1);
    emela_fiber_release(f);
    assert(!emela_task_complete_int(task, 0));
    if (mode < 2 || mode == 5) {
      assert(result.f0.owner);
      if (mode == 0) {
        assert(result.f0.length == 3 && memcmp(result.f0.data, "a\0b", 3) == 0);
        assert(result.f1.tag == 0 && result.f1.payload.v0.f0.owner);
        assert(result.f1.payload.v0.f0.length == 3 && memcmp(result.f1.payload.v0.f0.data, "xyz", 3) == 0);
      } else if (mode == 1) assert(result.f0.length == 4 && memcmp(result.f0.data, "none", 4) == 0 && result.f1.tag == 1);
      else {
        assert(result.f0.owner == owner0 && result.f1.payload.v0.f0.owner == owner1);
        assert(result.f0.length == 2 && memcmp(result.f0.data, "42", 2) == 0);
      }
      {release}(&result);
    } else assert(!result.f0.owner);
    assert(test_heap_empty());
  }
}
