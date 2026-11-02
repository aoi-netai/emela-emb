
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
    char text[] = { 'a', 0, 'b' }, note[] = "note";
    emela_type_{pair} input = { .f0 = { .data = text, .length = 3 }, .f1 = { .tag = 0, .payload.v0.f0 = {.data = note, .length = 4} } };
    emela_function_type_{callback} f = legacy_maker(42);
    emela_type_{packet} result = f.call(f.env, input);
    emela_embedded_release_function_{callback}(&f);
    memset(text, 'X', 3); memset(note, 'X', 4);
    assert(result.tag == 0 && result.payload.v0.f0.f0.owner);
    assert(result.payload.v0.f0.f0.length == 5 && memcmp(result.payload.v0.f0.f0.data, "a\0b42", 5) == 0);
    assert(result.payload.v0.f0.f1.tag == 0 && result.payload.v0.f0.f1.payload.v0.f0.owner);
    assert(memcmp(result.payload.v0.f0.f1.payload.v0.f0.data, "note", 4) == 0);
    {release}(&result);
    result = legacy_native(42);
    assert(result.tag == 0 && result.payload.v0.f0.f0.length == 4 && memcmp(result.payload.v0.f0.f0.data, "4342", 4) == 0);
    {release}(&result);
    assert(test_heap_empty());
  }
}
