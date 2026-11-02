
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask tasks[3];static int waits, marks;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
void board_wait_start(EmelaTask t,int32_t n) { assert(n>=1 && n<=3);tasks[n-1]=t;waits++; }
void mark(int32_t n) { assert(n==++marks); }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { fprintf(stderr,"%d: %.*s\n",k,n,m);abort(); }
int main(void) {
 for(int i=0;i<1000;i++) {
  waits=marks=0;int32_t result=-1;
  void *cause=NULL;
  EmelaFiber first=legacy_make_start(&cause);
  while(emela_scheduler_run(1)) {}
  assert(waits==2);assert(emela_task_complete_int(tasks[1],0));
  while(!emela_fiber_done(first)) assert(emela_scheduler_run(1));
  emela_fiber_release(first);
  char output[32];EmelaString text=legacy_show(cause,output,32);
  assert(text.length==11 && memcmp(text.data,"interrupted",11)==0);
  EmelaFiber f=legacy_raised_start(cause,i%2,&result);
  while(emela_scheduler_run(1)) {}
  if(i%2) { assert(marks==0 && !emela_fiber_done(f));assert(emela_task_complete_int(tasks[2],0)); }
  while(!emela_fiber_done(f)) assert(emela_scheduler_run(1));
  assert(result==42 && marks==3 && !emela_task_complete_int(tasks[0],0));
  emela_fiber_release(f);emela_embedded_release(cause);assert(test_heap_empty());
 }
}
