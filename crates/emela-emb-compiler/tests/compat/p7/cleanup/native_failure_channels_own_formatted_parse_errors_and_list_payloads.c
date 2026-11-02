#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static char *input;
static int cleanups;
int test_heap_empty(void);
void mutate(void) { memset(input, 'x', 5); cleanups++; }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k,n,m); abort(); }
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    assert(legacy_caught(INT32_MIN) == 12);
    assert(legacy_batch(INT32_MIN) == 12);
    assert(legacy_local(INT32_MIN) == 11);
    char bytes[] = {'a', 0, 'b', (char)0xc3, (char)0xa9}; input = bytes;
    EmelaString result = legacy_snapshot((EmelaString){.data = bytes, .length = 5});
    assert(cleanups == i * 2 + 1 && memcmp(bytes, "xxxxx", 5) == 0);
    assert(result.owner && result.length == 5 && memcmp(result.data, "a\0b\xc3\xa9", 5) == 0);
    emela_embedded_release_string(&result);
    memcpy(bytes, "abcde", 5);
    assert(legacy_captured((EmelaString){.data = bytes, .length = 5}) == 42);
    assert(cleanups == i * 2 + 2);
    assert(test_heap_empty());
  }
}
