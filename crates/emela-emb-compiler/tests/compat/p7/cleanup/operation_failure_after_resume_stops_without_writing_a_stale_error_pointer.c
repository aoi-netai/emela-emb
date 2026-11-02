#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) {
 const char *expected="operation `tick` of handler `Ticking` failed after resume: Boom";
 assert(k==EMELA_DEFECT_PANIC && n==strlen(expected) && memcmp(m,expected,n)==0); exit(0);
}
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
int main(void) {
 EmelaFiber f=legacy_work_start(NULL);
 while(!emela_fiber_done(f)) assert(emela_scheduler_run(1));
 abort();
}
