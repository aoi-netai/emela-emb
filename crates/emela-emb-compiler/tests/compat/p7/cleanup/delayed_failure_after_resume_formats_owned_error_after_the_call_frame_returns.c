#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask tasks[2]; static int32_t result=-99; static int phase, count;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t,int32_t n) { assert(n==1 || n==2);tasks[n-1]=t;count++; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) {
 const char *expected="operation `tick` of handler `Ticking` failed after resume: Boom(n: -2147483648)";
 assert(phase==1 && result==-99 && k==EMELA_DEFECT_PANIC && n==strlen(expected) && memcmp(m,expected,n)==0);exit(0);
}
int main(void) {
 EmelaFiber f=legacy_work_start(&result);
 while(emela_scheduler_run(1)) {} assert(!emela_fiber_done(f) && count==2);
 phase=1;assert(emela_task_complete_int(tasks[0],0));
 while(emela_scheduler_run(1)) {} abort();
}
