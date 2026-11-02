
#include <assert.h>
#include "emela_embedded.h"
int main(void) {
  char data[] = "abc";
  emela_type_{ty} input = {.f0 = {.data = data, .length = 3}, .f1 = {.tag = 1}};
  emela_type_{ty} result = legacy_identity(input);
  assert(result.f0.data == data && !result.f0.owner && result.f1.tag == 1);
}
