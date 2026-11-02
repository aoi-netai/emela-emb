#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask task;
static int released;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t,int32_t n) { (void)n;task=t; }
void mark(int32_t n) { assert(n==1); released++; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) {
 assert(k==EMELA_DEFECT_UNHANDLED_ERROR && n==15 && memcmp(m,"unhandled error",15)==0 && released==1); exit(0);
}
int main(void) {
 EmelaFiber f=legacy_failure_start(NULL);
 while(emela_scheduler_run(1)) {}
 assert(emela_task_complete_int(task,0));
 while(!emela_fiber_done(f)) assert(emela_scheduler_run(1));
 abort();
}
