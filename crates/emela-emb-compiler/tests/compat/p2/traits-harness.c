#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { fprintf(stderr, "%d: %.*s\n", k, n, m); abort(); }
static void equal_text(EmelaString actual, const char *expected, size_t length) {
  if (actual.length != length || memcmp(actual.data, expected, length) != 0) {
    fprintf(stderr, "got %.*s (%u), expected %.*s (%zu)\n", actual.length, actual.data, actual.length, (int)length, expected, length);
    abort();
  }
}
int main(void) {
  char output[256];
  const char expected[] = "(Envelope(key: K7, text: \"x\"), Some(K7), Present(K7, \"x\"))";
  static const char bytes[] = {'a', 0, '"', '\\', '\n', '#', '{', (char)0xce, (char)0xbb};
  const char escaped[] = "\"a\\u{0}\\\"\\\\\\n\\#{\xce\xbb\"";
  for (unsigned iteration = 0; iteration < 1000; iteration++) {
    EmelaString value = sync_traits(7, (EmelaString){"x", 1, NULL}, output, sizeof output);
    assert(value.data == output && !value.owner);
    equal_text(value, expected, sizeof expected - 1);
    equal_text(quoted((EmelaString){bytes, sizeof bytes, NULL}, output, sizeof output), escaped, sizeof escaped - 1);
    assert(trait_defaults() == 11);
    assert(trait_failure(7) == 7 && trait_failure(-7) == 93);
  }
  return 0;
}
