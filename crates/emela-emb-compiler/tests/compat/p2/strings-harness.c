#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  fprintf(stderr, "%d: %.*s\n", kind, length, message); abort();
}
static void check(EmelaString a, EmelaString b, int order) {
  assert(string_eq(a, b) == (order == 0));
  assert(string_ne(a, b) == (order != 0));
  assert(string_lt(a, b) == (order < 0));
  assert(string_le(a, b) == (order <= 0));
  assert(string_gt(a, b) == (order > 0));
  assert(string_ge(a, b) == (order >= 0));
}
int main(void) {
  const char a[] = {'a', 0, 'b'}, b[] = {'a', 0, 'c'}, same[] = {'a', 0, 'b'};
  check((EmelaString){NULL, 0, NULL}, (EmelaString){NULL, 0, NULL}, 0);
  check((EmelaString){NULL, 0, NULL}, (EmelaString){a, 3, NULL}, -1);
  check((EmelaString){a, 3, NULL}, (EmelaString){NULL, 0, NULL}, 1);
  check((EmelaString){a, 3, NULL}, (EmelaString){same, 3, NULL}, 0);
  check((EmelaString){a, 3, NULL}, (EmelaString){b, 3, NULL}, -1);
  check((EmelaString){a, 3, NULL}, (EmelaString){a, 1, NULL}, 1);
  check((EmelaString){"\xee\x80\x80", 3, NULL}, (EmelaString){"\xf0\x90\x80\x80", 4, NULL}, -1);
  check((EmelaString){"\x7f", 1, NULL}, (EmelaString){"\xc2\x80", 2, NULL}, -1);
  check((EmelaString){"e\xcc\x81", 3, NULL}, (EmelaString){"\xc3\xa9", 2, NULL}, -1);
  static char long_a[UINT16_MAX], long_b[UINT16_MAX];
  memset(long_a, 'a', sizeof long_a); memset(long_b, 'a', sizeof long_b);
  check((EmelaString){long_a, UINT16_MAX, NULL}, (EmelaString){long_b, UINT16_MAX, NULL}, 0);
  long_b[UINT16_MAX - 1] = 'b';
  check((EmelaString){long_a, UINT16_MAX, NULL}, (EmelaString){long_b, UINT16_MAX, NULL}, -1);
  assert(string_byte_size((EmelaString){long_a, UINT16_MAX, NULL}) == UINT16_MAX);
  assert(string_repeated((EmelaString){a, 3, NULL}, (EmelaString){same, 3, NULL}, 1000000));
  assert(!string_repeated((EmelaString){a, 3, NULL}, (EmelaString){b, 3, NULL}, 1000001));
  assert(string_predicates((EmelaString){a, 3, NULL}, (EmelaString){a + 1, 1, NULL}) == 4);
  assert(string_predicates((EmelaString){"émela", 6, NULL}, (EmelaString){"é", 2, NULL}) == 6);
  assert(string_predicates((EmelaString){"émela", 6, NULL}, (EmelaString){"la", 2, NULL}) == 5);
  assert(string_predicates((EmelaString){a, 3, NULL}, (EmelaString){NULL, 0, NULL}) == 7);
  assert(string_trimmed((EmelaString){" \témela\r\n", 10, NULL}, (EmelaString){"émela", 6, NULL}));
  assert(string_trimmed((EmelaString){NULL, 0, NULL}, (EmelaString){NULL, 0, NULL}));
  for (int iteration = 0; iteration < 1000; iteration++) {
    char output[64];
    EmelaString result = message((EmelaString){a, 3, NULL}, output, 64);
    assert(result.length == 18 && result.data == output && !result.owner);
    assert(memcmp(result.data, "Missing(text: a\0b)", 18) == 0);
  }
  return 0;
}
