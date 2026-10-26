#pragma once
#include <fltKernel.h>
#include <ntstrsafe.h>
#include "../shared/guard_policy.h"
#ifndef FOCUSER_EXPERIMENTAL_DRIVER
#error This uncertified driver may only be built as an explicitly experimental artifact.
#endif
#define FG_TAG 'gCoF'
extern PFLT_FILTER FgFilter;
extern FAST_MUTEX FgLock;
extern FG_STATE FgState;
extern PEPROCESS FgController;
extern UNICODE_STRING FgRoots[4];
BOOLEAN FgActive(void);
BOOLEAN FgIsController(void);
BOOLEAN FgWithin(PCUNICODE_STRING path, PCUNICODE_STRING root);
NTSTATUS FgRegisterTamper(PDRIVER_OBJECT driver);
void FgUnregisterTamper(void);
