/* EXPERIMENTAL: not certified, not included in the ordinary installer.
 * Build and OS acceptance gates are described in ../README.md. */
#include "guard.h"

PFLT_FILTER FgFilter;
FAST_MUTEX FgLock;
FG_STATE FgState;
PEPROCESS FgController;
UNICODE_STRING FgRoots[4];
static WCHAR RootStorage[4][1024];
static PFLT_PORT ServerPort, ClientPort;
static const UNICODE_STRING ServiceName = RTL_CONSTANT_STRING(L"FocuserAuthority");
static const UNICODE_STRING ServiceLeaf = RTL_CONSTANT_STRING(L"\\focuser-service.exe");
static BOOLEAN TamperRegistered;

static uint64_t ticks(void) { return KeQueryInterruptTime(); }
static uint64_t utc(void) {
    LARGE_INTEGER now;
    KeQuerySystemTimePrecise(&now);
    return now.QuadPart - 116444736000000000LL;
}
BOOLEAN FgActive(void) {
    BOOLEAN active;
    ExAcquireFastMutex(&FgLock);
    active = (BOOLEAN)fg_protected(&FgState, ticks());
    ExReleaseFastMutex(&FgLock);
    return active;
}
BOOLEAN FgIsController(void) {
    BOOLEAN same;
    ExAcquireFastMutex(&FgLock);
    same = FgController && PsGetCurrentProcess() == FgController;
    ExReleaseFastMutex(&FgLock);
    return same;
}
BOOLEAN FgWithin(PCUNICODE_STRING path, PCUNICODE_STRING root) {
    USHORT offset = root->Length / sizeof(WCHAR);
    return root->Length && path->Length >= root->Length && RtlPrefixUnicodeString(root, path, TRUE)
        && (path->Length == root->Length || path->Buffer[offset] == L'\\'
            || path->Buffer[offset] == L':');
}
static BOOLEAN guarded(PCUNICODE_STRING path, BOOLEAN ancestors) {
    ULONG i;
    for (i = 0; i < RTL_NUMBER_OF(FgRoots); ++i)
        if (FgWithin(path, &FgRoots[i]) || (ancestors && FgWithin(&FgRoots[i], path))) return TRUE;
    return FALSE;
}
static BOOLEAN trusted_controller(void) {
    PACCESS_TOKEN token;
    PTOKEN_USER user = NULL;
    PTOKEN_GROUPS groups = NULL;
    PUNICODE_STRING image = NULL;
    UCHAR serviceSid[SECURITY_MAX_SID_SIZE];
    ULONG sidLength = sizeof(serviceSid), i;
    BOOLEAN found = FALSE;
    UNICODE_STRING expected;
    WCHAR storage[1100];
    NTSTATUS status;
    if (PsGetProcessSessionId(PsGetCurrentProcess()) != 0 || PsIsThreadTerminating(PsGetCurrentThread())) return FALSE;
    status = RtlCreateServiceSid((PUNICODE_STRING)&ServiceName, (PSID)serviceSid, &sidLength);
    if (!NT_SUCCESS(status)) return FALSE;
    token = PsReferencePrimaryToken(PsGetCurrentProcess());
    status = SeQueryInformationToken(token, TokenUser, (PVOID*)&user);
    if (NT_SUCCESS(status) && RtlEqualSid(user->User.Sid, SeExports->SeLocalSystemSid)) {
        status = SeQueryInformationToken(token, TokenGroups, (PVOID*)&groups);
        if (NT_SUCCESS(status)) for (i = 0; i < groups->GroupCount; ++i)
            if ((groups->Groups[i].Attributes & SE_GROUP_ENABLED)
                && RtlEqualSid(groups->Groups[i].Sid, (PSID)serviceSid)) found = TRUE;
    }
    PsDereferencePrimaryToken(token);
    if (user) ExFreePool(user);
    if (groups) ExFreePool(groups);
    if (!found || !NT_SUCCESS(SeLocateProcessImageName(PsGetCurrentProcess(), &image))) return FALSE;
    RtlInitEmptyUnicodeString(&expected, storage, sizeof(storage));
    status = RtlUnicodeStringCopy(&expected, &FgRoots[0]);
    if (NT_SUCCESS(status)) status = RtlUnicodeStringCat(&expected, &ServiceLeaf);
    found = NT_SUCCESS(status) && RtlEqualUnicodeString(image, &expected, TRUE);
    ExFreePool(image);
    return found;
}
static NTSTATUS connect_port(PFLT_PORT client, PVOID serverCookie, PVOID context,
                             ULONG size, PVOID *cookie) {
    PEPROCESS old;
    UNREFERENCED_PARAMETER(serverCookie);
    UNREFERENCED_PARAMETER(context);
    /* Identity is obtained from the kernel token and image, never context. */
    if (size || !trusted_controller()) return STATUS_ACCESS_DENIED;
    ExAcquireFastMutex(&FgLock);
    old = FgController;
    FgController = PsGetCurrentProcess();
    ObReferenceObject(FgController);
    fg_new_connection(&FgState);
    ClientPort = client;
    *cookie = FgController;
    ExReleaseFastMutex(&FgLock);
    if (old) ObDereferenceObject(old);
    return STATUS_SUCCESS;
}
static void disconnect_port(PVOID cookie) {
    UNREFERENCED_PARAMETER(cookie);
    /* Keep process protection and the policy after the controller disappears. */
    FltCloseClientPort(FgFilter, &ClientPort);
}
static NTSTATUS message_port(PVOID cookie, PVOID input, ULONG inputLength,
                              PVOID output, ULONG outputLength, PULONG written) {
    FG_MESSAGE message;
    FG_REPLY reply;
    UNREFERENCED_PARAMETER(cookie);
    *written = 0;
    if (!FgIsController()) return STATUS_ACCESS_DENIED;
    if (inputLength != sizeof(message) || outputLength != sizeof(reply)) return STATUS_INFO_LENGTH_MISMATCH;
    __try {
        ProbeForRead(input, inputLength, 1);
        RtlCopyMemory(&message, input, sizeof(message));
        ExAcquireFastMutex(&FgLock);
        fg_dispatch(&FgState, &message, sizeof(message), utc(), ticks(), &reply);
        ExReleaseFastMutex(&FgLock);
        ProbeForWrite(output, outputLength, 1);
        RtlCopyMemory(output, &reply, sizeof(reply));
        *written = sizeof(reply);
    } __except(EXCEPTION_EXECUTE_HANDLER) { return GetExceptionCode(); }
    return STATUS_SUCCESS;
}
static FLT_PREOP_CALLBACK_STATUS deny(PFLT_CALLBACK_DATA data) {
    data->IoStatus.Status = STATUS_ACCESS_DENIED;
    data->IoStatus.Information = 0;
    return FLT_PREOP_COMPLETE;
}
static FLT_PREOP_CALLBACK_STATUS pre_operation(PFLT_CALLBACK_DATA data,
                                              PCFLT_RELATED_OBJECTS objects,
                                              PVOID *completion) {
    PFLT_FILE_NAME_INFORMATION name = NULL, destination = NULL;
    BOOLEAN ancestor = FALSE, mutation = TRUE, blocked = FALSE;
    NTSTATUS status;
    UNREFERENCED_PARAMETER(completion);
    if (!FgActive() || FgIsController()) return FLT_PREOP_SUCCESS_NO_CALLBACK;
    if (data->Iopb->MajorFunction == IRP_MJ_CREATE) {
        ACCESS_MASK access = data->Iopb->Parameters.Create.SecurityContext->DesiredAccess;
        ULONG options = data->Iopb->Parameters.Create.Options;
        ULONG disposition = options >> 24;
        mutation = (access & (FILE_WRITE_DATA | FILE_APPEND_DATA | FILE_WRITE_EA
                      | FILE_WRITE_ATTRIBUTES | DELETE | WRITE_DAC | WRITE_OWNER))
            || (options & FILE_DELETE_ON_CLOSE) || disposition == FILE_SUPERSEDE
            || disposition == FILE_OVERWRITE || disposition == FILE_OVERWRITE_IF;
        ancestor = (access & (DELETE | WRITE_DAC | WRITE_OWNER)) || (options & FILE_DELETE_ON_CLOSE);
    } else if (data->Iopb->MajorFunction == IRP_MJ_SET_INFORMATION) {
        FILE_INFORMATION_CLASS kind = data->Iopb->Parameters.SetFileInformation.FileInformationClass;
        ancestor = kind == FileDispositionInformation || kind == FileDispositionInformationEx
            || kind == FileRenameInformation || kind == FileRenameInformationEx;
        if (kind == FileRenameInformation || kind == FileRenameInformationEx
            || kind == FileLinkInformation || kind == FileLinkInformationEx) {
            PFILE_RENAME_INFORMATION rename = data->Iopb->Parameters.SetFileInformation.InfoBuffer;
            ULONG length = data->Iopb->Parameters.SetFileInformation.Length;
            if (length < FIELD_OFFSET(FILE_RENAME_INFORMATION, FileName)
                || rename->FileNameLength > length - FIELD_OFFSET(FILE_RENAME_INFORMATION, FileName)
                || (rename->FileNameLength & 1)) return deny(data);
            status = FltGetDestinationFileNameInformation(objects->Instance, objects->FileObject,
                rename->RootDirectory, rename->FileName, rename->FileNameLength,
                FLT_FILE_NAME_NORMALIZED | FLT_FILE_NAME_QUERY_DEFAULT, &destination);
            if (!NT_SUCCESS(status)) return deny(data);
            blocked = guarded(&destination->Name, FALSE);
            FltReleaseFileNameInformation(destination);
        }
    } else if (data->Iopb->MajorFunction == IRP_MJ_SET_SECURITY) ancestor = TRUE;
    if (!mutation) return FLT_PREOP_SUCCESS_NO_CALLBACK;
    status = FltGetFileNameInformation(data, FLT_FILE_NAME_NORMALIZED | FLT_FILE_NAME_QUERY_DEFAULT, &name);
    if (NT_SUCCESS(status)) {
        blocked |= guarded(&name->Name, ancestor);
        FltReleaseFileNameInformation(name);
    } else {
        /* Never freeze unrelated system writes when a name isn't available.
           This coverage gap is a RELEASE BLOCKER, documented in README. */
        return blocked ? deny(data) : FLT_PREOP_SUCCESS_NO_CALLBACK;
    }
    return blocked ? deny(data) : FLT_PREOP_SUCCESS_NO_CALLBACK;
}
static NTSTATUS instance_setup(PCFLT_RELATED_OBJECTS objects, FLT_INSTANCE_SETUP_FLAGS flags,
                               DEVICE_TYPE device, FLT_FILESYSTEM_TYPE filesystem) {
    UNREFERENCED_PARAMETER(objects); UNREFERENCED_PARAMETER(flags);
    return device == FILE_DEVICE_DISK_FILE_SYSTEM && filesystem == FLT_FSTYPE_NTFS
        ? STATUS_SUCCESS : STATUS_FLT_DO_NOT_ATTACH;
}
static NTSTATUS instance_teardown(PCFLT_RELATED_OBJECTS objects, FLT_INSTANCE_QUERY_TEARDOWN_FLAGS flags) {
    UNREFERENCED_PARAMETER(objects); UNREFERENCED_PARAMETER(flags);
    return FgActive() ? STATUS_FLT_DO_NOT_DETACH : STATUS_SUCCESS;
}
static NTSTATUS unload(FLT_FILTER_UNLOAD_FLAGS flags) {
    BOOLEAN allowed;
    UNREFERENCED_PARAMETER(flags);
    ExAcquireFastMutex(&FgLock);
    allowed = (BOOLEAN)fg_may_unload(&FgState, ticks());
    ExReleaseFastMutex(&FgLock);
    if (!allowed) return STATUS_FLT_DO_NOT_DETACH;
    FltCloseCommunicationPort(ServerPort);
    if (TamperRegistered) FgUnregisterTamper();
    FltUnregisterFilter(FgFilter);
    if (FgController) ObDereferenceObject(FgController);
    return STATUS_SUCCESS;
}
static const FLT_OPERATION_REGISTRATION Operations[] = {
    { IRP_MJ_CREATE, 0, pre_operation, NULL },
    { IRP_MJ_WRITE, 0, pre_operation, NULL },
    { IRP_MJ_SET_INFORMATION, 0, pre_operation, NULL },
    { IRP_MJ_SET_SECURITY, 0, pre_operation, NULL },
    { IRP_MJ_FILE_SYSTEM_CONTROL, 0, pre_operation, NULL },
    { IRP_MJ_OPERATION_END }
};
static const FLT_REGISTRATION Registration = {
    sizeof(FLT_REGISTRATION), FLT_REGISTRATION_VERSION,
    FLTFL_REGISTRATION_DO_NOT_SUPPORT_SERVICE_STOP, NULL, Operations,
    unload, instance_setup, instance_teardown, NULL, NULL,
    NULL, NULL, NULL, NULL, NULL, NULL
};
static NTSTATUS read_value(HANDLE key, PCWSTR name, ULONG type, PVOID output, ULONG bytes) {
    UCHAR buffer[2304];
    PKEY_VALUE_PARTIAL_INFORMATION value = (PKEY_VALUE_PARTIAL_INFORMATION)buffer;
    UNICODE_STRING valueName;
    ULONG returned;
    NTSTATUS status;
    RtlInitUnicodeString(&valueName, name);
    status = ZwQueryValueKey(key, &valueName, KeyValuePartialInformation, value, sizeof(buffer), &returned);
    if (!NT_SUCCESS(status)) return status;
    if (value->Type != type || value->DataLength > bytes
        || (type == REG_BINARY && value->DataLength != bytes)
        || (type == REG_SZ && (value->DataLength & 1))) return STATUS_INVALID_PARAMETER;
    RtlZeroMemory(output, bytes);
    RtlCopyMemory(output, value->Data, value->DataLength);
    return STATUS_SUCCESS;
}
static NTSTATUS initialize_own_image(PDRIVER_OBJECT driver) {
    UNICODE_STRING loaded = {0};
    OBJECT_ATTRIBUTES attributes;
    IO_STATUS_BLOCK io;
    HANDLE handle = NULL;
    PFILE_OBJECT file = NULL;
    PFLT_FILE_NAME_INFORMATION normalized = NULL;
    NTSTATUS status = IoQueryFullDriverPath(driver, &loaded);
    if (!NT_SUCCESS(status)) return status;
    InitializeObjectAttributes(&attributes, &loaded, OBJ_KERNEL_HANDLE | OBJ_CASE_INSENSITIVE, NULL, NULL);
    status = ZwCreateFile(&handle, FILE_READ_ATTRIBUTES | SYNCHRONIZE, &attributes,
        &io, NULL, 0, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        FILE_OPEN, FILE_SYNCHRONOUS_IO_NONALERT | FILE_NON_DIRECTORY_FILE, NULL, 0);
    ExFreePool(loaded.Buffer);
    if (!NT_SUCCESS(status)) return status;
    status = ObReferenceObjectByHandle(handle, FILE_READ_ATTRIBUTES, *IoFileObjectType,
        KernelMode, (PVOID*)&file, NULL);
    if (NT_SUCCESS(status)) {
        status = FltGetFileNameInformationUnsafe(file, NULL,
            FLT_FILE_NAME_NORMALIZED | FLT_FILE_NAME_QUERY_DEFAULT, &normalized);
        if (NT_SUCCESS(status)) {
            RtlInitEmptyUnicodeString(&FgRoots[3], RootStorage[3], sizeof(RootStorage[3]));
            status = RtlUnicodeStringCopy(&FgRoots[3], &normalized->Name);
            FltReleaseFileNameInformation(normalized);
        }
        ObDereferenceObject(file);
    }
    ZwClose(handle);
    return status;
}
DRIVER_INITIALIZE DriverEntry;
NTSTATUS DriverEntry(PDRIVER_OBJECT driver, PUNICODE_STRING registryPath) {
    HANDLE key = NULL;
    OBJECT_ATTRIBUTES attributes;
    UNICODE_STRING path, portName;
    WCHAR parameters[1024];
    UCHAR installation[16];
    PSECURITY_DESCRIPTOR security = NULL;
    NTSTATUS status;
    ULONG i;
    const PCWSTR names[] = { L"InstallRoot", L"AuthorityRoot", L"TasksRoot" };
    const PCWSTR suffixes[] = { L"\\Program Files\\Focuser", L"\\ProgramData\\FocuserAuthority", L"\\Windows\\System32\\Tasks\\Focuser" };
    ExInitializeFastMutex(&FgLock);
    RtlInitEmptyUnicodeString(&path, parameters, sizeof(parameters));
    status = RtlUnicodeStringCopy(&path, registryPath);
    if (NT_SUCCESS(status)) status = RtlUnicodeStringCatString(&path, L"\\Parameters");
    if (!NT_SUCCESS(status)) return status;
    InitializeObjectAttributes(&attributes, &path, OBJ_KERNEL_HANDLE | OBJ_CASE_INSENSITIVE, NULL, NULL);
    status = ZwOpenKey(&key, KEY_QUERY_VALUE, &attributes);
    if (!NT_SUCCESS(status)) return status;
    status = read_value(key, L"Installation", REG_BINARY, installation, sizeof(installation));
    for (i = 0; NT_SUCCESS(status) && i < RTL_NUMBER_OF(names); ++i) {
        UNICODE_STRING prefix = RTL_CONSTANT_STRING(L"\\Device\\");
        UNICODE_STRING suffix, expected;
        USHORT at;
        status = read_value(key, names[i], REG_SZ, RootStorage[i], sizeof(RootStorage[i]) - sizeof(WCHAR));
        if (!NT_SUCCESS(status)) break;
        RtlInitUnicodeString(&FgRoots[i], RootStorage[i]);
        if (!RtlPrefixUnicodeString(&prefix, &FgRoots[i], TRUE)
            || FgRoots[i].Length < 32 || FgRoots[i].Buffer[FgRoots[i].Length / sizeof(WCHAR) - 1] == L'\\') status = STATUS_INVALID_PARAMETER;
        if (!NT_SUCCESS(status)) break;
        /* Fixed product roots only. Even an enrollment parameter cannot turn
           this into an arbitrary-path denial driver. Custom layouts unsupported. */
        at = prefix.Length / sizeof(WCHAR);
        while (at < FgRoots[i].Length / sizeof(WCHAR) && FgRoots[i].Buffer[at] != L'\\') ++at;
        if (at == prefix.Length / sizeof(WCHAR)) { status = STATUS_INVALID_PARAMETER; break; }
        suffix = FgRoots[i];
        suffix.Buffer += at;
        suffix.Length -= at * sizeof(WCHAR);
        suffix.MaximumLength = suffix.Length;
        RtlInitUnicodeString(&expected, suffixes[i]);
        if (!RtlEqualUnicodeString(&suffix, &expected, TRUE)) status = STATUS_INVALID_PARAMETER;
    }
    ZwClose(key);
    if (!NT_SUCCESS(status)) return status;
    fg_initialize(&FgState, installation);
    status = FltRegisterFilter(driver, &Registration, &FgFilter);
    if (!NT_SUCCESS(status)) return status;
    status = initialize_own_image(driver);
    if (!NT_SUCCESS(status)) goto failed;
    status = FgRegisterTamper(driver);
    if (!NT_SUCCESS(status)) goto failed;
    TamperRegistered = TRUE;
    status = FltBuildDefaultSecurityDescriptor(&security, FLT_PORT_ALL_ACCESS);
    if (!NT_SUCCESS(status)) goto failed;
    RtlInitUnicodeString(&portName, FG_PORT_NAME);
    InitializeObjectAttributes(&attributes, &portName, OBJ_KERNEL_HANDLE | OBJ_CASE_INSENSITIVE, NULL, security);
    status = FltCreateCommunicationPort(FgFilter, &ServerPort, &attributes, NULL,
                                      connect_port, disconnect_port, message_port, 1);
    FltFreeSecurityDescriptor(security);
    if (!NT_SUCCESS(status)) goto failed;
    status = FltStartFiltering(FgFilter);
    if (NT_SUCCESS(status)) return status;
    FltCloseCommunicationPort(ServerPort);
failed:
    if (TamperRegistered) FgUnregisterTamper();
    FltUnregisterFilter(FgFilter);
    return status;
}
