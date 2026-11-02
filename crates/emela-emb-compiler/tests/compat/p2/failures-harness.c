#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
static volatile unsigned char saved;
void board_smash(void) {
  volatile unsigned char scratch[4096];
  for (unsigned i = 0; i < sizeof scratch; i++) scratch[i] = (unsigned char)i;
  saved = scratch[sizeof scratch - 1];
}
int main(void) {
  assert(caught_value(7) == 16 && caught_value(-7) == 2);
  assert(caught_value(INT32_MIN) == 11);
  assert(caught_text_matches(INT32_MIN));
  assert(caught_text_matches(INT32_MAX));
  assert(caught_text_matches(0));
  const char original[] = {'a', 0, 'b'};
  assert(original_error_input_matches((EmelaString){original, 3, NULL}));
  assert(nested_catch(7) == 8 && nested_catch(-7) == 12);
  assert(guarded_catch(7) == 8);
  assert(guarded_catch(-7) == 2 && guarded_catch(-700) == 100);
  return 0;
}
