#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_cancel(EmelaFiber f);
static EmelaTask request, plain;
static int locked, isr, produced, released, marks;
typedef struct { char message[3]; void *parts; } Context;
uint32_t emela_board_event_lock(void) { int old=locked; locked=1; return old; }
void emela_board_event_unlock(uint32_t p) { assert(locked); locked=(int)p; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) { fprintf(stderr,"%d:%.*s\n",k,n,m); abort(); }
void board_request_start(EmelaTask t,int32_t n) { (void)n;request=t; }
void board_wait_start(EmelaTask t,int32_t n) { (void)n;plain=t; }
void mark(int32_t n) { assert(n==1 && !isr && !locked); marks++; }
static void produce(void *p,EmelaError *error) {
  Context *c=p;assert(!isr && !locked);produced++;
  *error=(EmelaError){.tag={tag}+1,.payload.e{tag}=(emela_type_{ty}){.tag=0,.payload.v0={
    .f0={.data=c->message,.length=3},.f1=c->parts
  }}};
}
static void release(void *p) { Context*c=p;assert(!isr && !locked);released++;memset(c->message,'x',3);emela_embedded_release(c->parts);c->parts=NULL; }
static void drain(void) { while(emela_scheduler_run(1)) {} }
static void fail(Context *c) { isr=1;assert(emela_task_fail(request,produce,c,release));
  assert(!emela_task_fail(request,produce,c,release));assert(!emela_task_complete_int(request,99));
  assert(!produced && !released);isr=0; }
int main(void) {
  for(int i=0;i<1000;i++) {
    Context c={.message={'a','\0','b'},.parts=legacy_parts(42)};
    produced=released=marks=0;int32_t result=-1;
    EmelaFiber f=legacy_caught_start(i,&result);drain();fail(&c);drain();
    assert(emela_fiber_done(f)&&result==5&&produced==1&&released==1&&marks==1);
    emela_fiber_release(f);assert(test_heap_empty());

    c=(Context){.message={'a','b','c'},.parts=legacy_parts(42)};produced=released=0;
    EmelaString text={0};f=legacy_attempted_start(i,i%2?NULL:&text);drain();fail(&c);drain();
    assert(emela_fiber_done(f)&&produced==1&&released==1);emela_fiber_release(f);
    if(!(i%2)) { const char expected[]="error: Boom(message: \"abc\", parts: [\"42\"])";
      assert(text.length==strlen(expected)&&!memcmp(text.data,expected,text.length));emela_embedded_release(text.owner); }
    assert(test_heap_empty());

    c=(Context){.message={'a','b','c'},.parts=legacy_parts(42)};produced=released=0;result=-1;
    f=legacy_scoped_start(i,&result);drain();fail(&c);drain();
    assert(emela_fiber_done(f)&&result==3&&produced==1&&released==1);emela_fiber_release(f);assert(test_heap_empty());

    c=(Context){.message={'a','b','c'},.parts=legacy_parts(42)};produced=released=marks=0;result=-1;
    f=legacy_caught_start(i,&result);drain();fail(&c);test_cancel(f);
    (void)emela_scheduler_run(0);assert(!produced&&!released);
    assert(!test_heap_empty());drain();assert(emela_fiber_done(f)&&!produced&&released==1&&marks==1&&result==-1);emela_fiber_release(f);assert(test_heap_empty());
    assert(!emela_task_fail(request,produce,&c,release));

    produced=released=marks=0;result=-1;f=legacy_caught_start(i,&result);drain();test_cancel(f);drain();
    assert(emela_fiber_done(f)&&!emela_task_fail(request,produce,&c,release));emela_fiber_release(f);assert(test_heap_empty());

    produced=released=marks=0;result=-1;f=legacy_caught_start(i,&result);drain();
    assert(emela_task_complete_int(request,42));drain();assert(emela_fiber_done(f)&&result==42&&marks==1);emela_fiber_release(f);assert(test_heap_empty());

    f=legacy_plain_start(i,&result);drain();assert(!emela_task_fail(plain,produce,&c,release));assert(emela_task_complete_int(plain,42));drain();assert(result==42);emela_fiber_release(f);
    assert(!emela_task_fail((EmelaTask){UINT32_MAX,0},produce,&c,release));
    assert(!emela_task_fail(request,NULL,&c,release));assert(!produced&&!released&&test_heap_empty());
  }
}
