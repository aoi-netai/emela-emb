
#include <assert.h>
#include "emela_embedded.h"
int main(void) { for(int round=0;round<1000;round++){
  EmelaString both = {.data = "aba", .length = 3};
  EmelaString middle = {.data = "bab", .length = 3};
  EmelaString empty = {.data = "", .length = 0};
  assert(legacy_has_a(both) && legacy_starts_a(both) && legacy_ends_a(both));
  assert(legacy_has_a(middle) && !legacy_starts_a(middle) && !legacy_ends_a(middle));
  assert(!legacy_has_a(empty) && !legacy_starts_a(empty) && !legacy_ends_a(empty));
  }return 0;
}
