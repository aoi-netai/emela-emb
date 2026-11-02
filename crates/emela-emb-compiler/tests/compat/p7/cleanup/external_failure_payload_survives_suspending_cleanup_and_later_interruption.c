#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_cancel(EmelaFiber f);
static EmelaTask request,cleanup;
static int produced,released,marks;
typedef struct { char message[3];void *parts; } Context;
uint32_t emela_board_event_lock(void) { return 0; }
void emela_board_event_unlock(uint32_t p) { (void)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) { fprintf(stderr,"%d:%.*s\n",k,n,m);abort(); }
void board_request_start(EmelaTask t,int32_t n) { (void)n;request=t; }
void board_wait_start(EmelaTask t,int32_t n) { assert(n==99);cleanup=t; }
void mark(int32_t n) { assert(n==2);marks++; }
static void produce(void*p,EmelaError*e) { Context*c=p;produced++;
  *e=(EmelaError){.tag={tag}+1,.payload.e{tag}=(emela_type_{ty}){.tag=0,.payload.v0={
    .f0={.data=c->message,.length=3},.f1=c->parts
  }}};
}
static void release(void*p) { Context*c=p;released++;memset(c->message,'x',3);emela_embedded_release(c->parts); }
static void drain(void) { while(emela_scheduler_run(1)) {} }
int main(void) {
  for(int i=0;i<1000;i++) {
    Context c={.message={'a','\0','b'},.parts=legacy_parts(42)};
    int32_t result=-1;produced=released=marks=0;
    EmelaFiber f=legacy_cleaned_start(i,i%3?&result:NULL);drain();
    assert(emela_task_fail(request,produce,&c,release));drain();
    assert(produced==1&&released==1&&!marks&&!emela_fiber_done(f)&&!test_heap_empty());
    if(i%2) { test_cancel(f);drain();assert(!emela_fiber_done(f)); }
    assert(emela_task_complete_int(cleanup,0));drain();
    assert(emela_fiber_done(f)&&marks==1);emela_fiber_release(f);
    assert(result==((i%3&&!(i%2))?5:-1)&&test_heap_empty());
  }
}
