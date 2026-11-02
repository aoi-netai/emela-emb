#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask tasks[4]; static int expected_defect;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t,int32_t n) { assert(n>=1 && n<=4);tasks[n-1]=t; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) {
 assert(expected_defect && k==EMELA_DEFECT_STRING_CAPACITY && n==24 && memcmp(m,"string capacity exceeded",24)==0);exit(0);
}
int main(void) {
 void *cause=NULL;EmelaFiber f=legacy_work_start(&cause);
 while(emela_scheduler_run(1)) {}
 for(int i=0;i<3;i++) { assert(emela_task_complete_int(tasks[i],0));while(emela_scheduler_run(1)) {} }
 assert(emela_fiber_done(f) && cause);emela_fiber_release(f);
 char buffer[64];expected_defect=1;legacy_show(cause,buffer,sizeof(buffer));abort();
}
