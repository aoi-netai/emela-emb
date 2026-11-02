
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t, int32_t n) { (void)t; (void)n; abort(); }
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k,n,m); abort(); }
int main(void) {
  for (int i = 0; i < 1000; ++i) {
    char bytes[] = "abc";
    emela_function_type_{callback} f = legacy_maker((EmelaString){.data = bytes, .length = 3}, 42);
    memset(bytes, 'x', 3);
    emela_embedded_retain_function_{callback}(&f);
    emela_function_type_{callback} copy = f;
    emela_embedded_release_function_{callback}(&f);
    EmelaString value = copy.call(copy.env);
    emela_embedded_release_function_{callback}(&copy);
    assert(value.owner && value.length == 11 && memcmp(value.data, "abc42abc/43", 11) == 0);
    emela_embedded_release_string(&value);
    assert(test_heap_empty());
  }
}
