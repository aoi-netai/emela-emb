
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
int main(void) {
  for (int i = 0; i < 1000; i++) {
    emela_type_{ty} value = legacy_encoded(i % 2 ? 0x1f600 : 0);
    assert(value.tag == 0 && value.payload.v0.f0.owner);
    assert(value.payload.v0.f0.length == (i % 2 ? 4 : 1));
    assert(memcmp(value.payload.v0.f0.data, i % 2 ? "\xf0\x9f\x98\x80" : "\0", value.payload.v0.f0.length) == 0);
    {release}(&value);
    value = legacy_encoded(0xd800);
    assert(value.tag == 1); {release}(&value);
    assert(test_heap_empty());
  }
}
