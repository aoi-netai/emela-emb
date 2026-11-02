
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
void mark(int32_t n) { (void)n;abort(); }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) {
 assert(k==EMELA_DEFECT_PANIC && n==11 && memcmp(m,"test defect",11)==0);exit(0);
}
int main(void) { (void)legacy_stopped();abort(); }
