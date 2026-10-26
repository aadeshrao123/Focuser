#include "guard.h"
static PVOID ObjectCookie;
static LARGE_INTEGER RegistryCookie;
static BOOLEAN RegistryRegistered;
static BOOLEAN registry_ours(PCUNICODE_STRING name) {
    const UNICODE_STRING machine = RTL_CONSTANT_STRING(L"\\REGISTRY\\MACHINE\\SYSTEM\\");
    const UNICODE_STRING authority = RTL_CONSTANT_STRING(L"\\Services\\FocuserAuthority");
    const UNICODE_STRING guard = RTL_CONSTANT_STRING(L"\\Services\\FocuserGuard");
    const UNICODE_STRING task = RTL_CONSTANT_STRING(L"\\REGISTRY\\MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Schedule\\TaskCache\\Tree\\Focuser");
    USHORT offset;
    if (FgWithin(name, &task)) return TRUE;
    if (!RtlPrefixUnicodeString(&machine, name, TRUE)) return FALSE;
    for (offset = machine.Length / sizeof(WCHAR); offset < name->Length / sizeof(WCHAR); ++offset) {
        UNICODE_STRING suffix = *name;
        suffix.Buffer += offset;
        suffix.Length -= offset * sizeof(WCHAR);
        suffix.MaximumLength = suffix.Length;
        if (FgWithin(&suffix, &authority) || FgWithin(&suffix, &guard)) return TRUE;
    }
    return FALSE;
}
static NTSTATUS registry_callback(PVOID context, PVOID argument1, PVOID argument2) {
    REG_NOTIFY_CLASS kind = (REG_NOTIFY_CLASS)(ULONG_PTR)argument1;
    PCUNICODE_STRING name;
    PVOID object;
    BOOLEAN blocked;
    NTSTATUS status;
    UNREFERENCED_PARAMETER(context);
    if (!FgActive() || FgIsController()) return STATUS_SUCCESS;
    switch (kind) {
    case RegNtPreSetValueKey: object = ((PREG_SET_VALUE_KEY_INFORMATION)argument2)->Object; break;
    case RegNtPreDeleteValueKey: object = ((PREG_DELETE_VALUE_KEY_INFORMATION)argument2)->Object; break;
    case RegNtPreDeleteKey: object = ((PREG_DELETE_KEY_INFORMATION)argument2)->Object; break;
    case RegNtPreRenameKey: object = ((PREG_RENAME_KEY_INFORMATION)argument2)->Object; break;
    case RegNtPreSetKeySecurity: object = ((PREG_SET_KEY_SECURITY_INFORMATION)argument2)->Object; break;
    default: return STATUS_SUCCESS;
    }
    status = CmCallbackGetKeyObjectIDEx(&RegistryCookie, object, NULL, &name, 0);
    if (!NT_SUCCESS(status)) return status;
    blocked = registry_ours(name);
    CmCallbackReleaseKeyObjectIDEx(name);
    return blocked ? STATUS_ACCESS_DENIED : STATUS_SUCCESS;
}
static OB_PREOP_CALLBACK_STATUS object_callback(PVOID context, POB_PRE_OPERATION_INFORMATION info) {
    PEPROCESS owner;
    ACCESS_MASK *access;
    BOOLEAN same;
    UNREFERENCED_PARAMETER(context);
    if (info->KernelHandle || !FgActive() || FgIsController()) return OB_PREOP_SUCCESS;
    owner = info->ObjectType == *PsProcessType ? (PEPROCESS)info->Object
        : PsGetThreadProcess((PETHREAD)info->Object);
    ExAcquireFastMutex(&FgLock);
    same = owner == FgController;
    ExReleaseFastMutex(&FgLock);
    if (!same) return OB_PREOP_SUCCESS;
    access = info->Operation == OB_OPERATION_HANDLE_CREATE
        ? &info->Parameters->CreateHandleInformation.DesiredAccess
        : &info->Parameters->DuplicateHandleInformation.DesiredAccess;
    if (info->ObjectType == *PsProcessType)
        *access &= ~(PROCESS_TERMINATE | PROCESS_CREATE_THREAD | PROCESS_CREATE_PROCESS | PROCESS_VM_OPERATION
            | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_DUP_HANDLE | PROCESS_SET_INFORMATION
            | PROCESS_SET_QUOTA | PROCESS_SUSPEND_RESUME);
    else *access &= ~(THREAD_TERMINATE | THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_SET_CONTEXT
            | THREAD_SET_INFORMATION | THREAD_SET_THREAD_TOKEN | THREAD_DIRECT_IMPERSONATION | THREAD_IMPERSONATE);
    return OB_PREOP_SUCCESS;
}
NTSTATUS FgRegisterTamper(PDRIVER_OBJECT driver) {
    /* Altitude is an assigned release input, not a made-up production altitude. */
    UNICODE_STRING altitude = RTL_CONSTANT_STRING(FOCUSER_GUARD_ALTITUDE);
    OB_OPERATION_REGISTRATION operations[2] = {0};
    OB_CALLBACK_REGISTRATION registration = {0};
    NTSTATUS status;
    operations[0].ObjectType = PsProcessType;
    operations[1].ObjectType = PsThreadType;
    operations[0].Operations = operations[1].Operations = OB_OPERATION_HANDLE_CREATE | OB_OPERATION_HANDLE_DUPLICATE;
    operations[0].PreOperation = operations[1].PreOperation = object_callback;
    registration.Version = OB_FLT_REGISTRATION_VERSION;
    registration.OperationRegistrationCount = RTL_NUMBER_OF(operations);
    registration.Altitude = altitude;
    registration.OperationRegistration = operations;
    status = ObRegisterCallbacks(&registration, &ObjectCookie);
    if (!NT_SUCCESS(status)) return status;
    status = CmRegisterCallbackEx(registry_callback, &altitude, driver, NULL, &RegistryCookie, NULL);
    if (!NT_SUCCESS(status)) { ObUnRegisterCallbacks(ObjectCookie); ObjectCookie = NULL; return status; }
    RegistryRegistered = TRUE;
    return STATUS_SUCCESS;
}
void FgUnregisterTamper(void) {
    if (RegistryRegistered) { CmUnRegisterCallback(RegistryCookie); RegistryRegistered = FALSE; }
    if (ObjectCookie) { ObUnRegisterCallbacks(ObjectCookie); ObjectCookie = NULL; }
}
