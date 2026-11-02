
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
    char buffer[64];
    EmelaString value = legacy_checked(42, i % 2, buffer, 64);
    assert(value.length == (i % 2 ? 2 : 4) && memcmp(value.data, i % 2 ? "42" : "4343", value.length) == 0);
    emela_embedded_release_string(&value);
    assert(test_heap_empty());
  }
}
