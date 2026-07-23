#ifndef NUPA_HELLO_NP_H
#define NUPA_HELLO_NP_H

#include <nupa/object.h>

struct nupa_NPObject_vtable;
struct nupa_Student_vtable;

struct NPObject;
NPObject * NPObject_init(NPObject * self, SEL _cmd);
void NPObject_dealloc(NPObject * self, SEL _cmd);
struct nupa_NPObject_vtable;
struct Student;
int Student_grade(NPObject * self, SEL _cmd);
void Student_setGrade_(NPObject * self, SEL _cmd, int value);
struct nupa_Student_vtable;
extern NPClass nupa_NPObject_class;
extern NPClass nupa_Student_class;
void nupa_init(void);

#endif /* NUPA_HELLO_NP_H */
