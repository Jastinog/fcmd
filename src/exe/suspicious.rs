//! Curated set of "interesting" imported APIs for malware triage.
//!
//! Deliberately high-signal: process injection, dynamic code execution, anti-
//! analysis, privilege manipulation, in-memory crypto, and staged download — the
//! calls whose mere presence narrows a verdict. Generic file/socket primitives
//! (`WriteFile`, `connect`, `send`) are intentionally excluded; flagging those
//! would mark almost every binary and drown the real signal.

/// Canonical lowercased API names, without the Win32 `A`/`W` suffix (stripped
/// before lookup). Covers both Windows and the common Unix libc tells.
const SUSPICIOUS: &[&str] = &[
    // Memory allocation / protection used for injected or unpacked code.
    "virtualalloc",
    "virtualallocex",
    "virtualprotect",
    "virtualprotectex",
    "ntallocatevirtualmemory",
    "ntprotectvirtualmemory",
    // Cross-process read/write + remote execution (classic injection chain).
    "writeprocessmemory",
    "readprocessmemory",
    "ntwritevirtualmemory",
    "createremotethread",
    "createremotethreadex",
    "rtlcreateuserthread",
    "queueuserapc",
    "ntqueueapcthread",
    "setthreadcontext",
    "getthreadcontext",
    "resumethread",
    "ntunmapviewofsection",
    "zwunmapviewofsection",
    "mapviewoffile",
    "createfilemapping",
    "ntmapviewofsection",
    "openprocess",
    "openthread",
    "terminateprocess",
    // Dynamic resolution / loading (hides intent from static import view).
    "loadlibrary",
    "loadlibraryex",
    "ldrloaddll",
    "getprocaddress",
    "getmodulehandle",
    // Process / command execution.
    "winexec",
    "createprocess",
    "createprocessinternal",
    "shellexecute",
    "shellexecuteex",
    // Hooking / input capture (keyloggers).
    "setwindowshookex",
    "getasynckeystate",
    "getkeyboardstate",
    "registerrawinputdevices",
    // Anti-debug / anti-analysis.
    "isdebuggerpresent",
    "checkremotedebuggerpresent",
    "ntqueryinformationprocess",
    "outputdebugstring",
    "ntsetinformationthread",
    // Privilege escalation / token manipulation.
    "adjusttokenprivileges",
    "openprocesstoken",
    "lookupprivilegevalue",
    "duplicatetokenex",
    // Persistence via registry autoruns.
    "regsetvalueex",
    "regcreatekeyex",
    // In-memory crypto (packed payloads / ransomware).
    "cryptencrypt",
    "cryptdecrypt",
    "cryptacquirecontext",
    "cryptgenkey",
    "bcryptencrypt",
    "bcryptdecrypt",
    // Staged download.
    "urldownloadtofile",
    "internetopenurl",
    "internetconnect",
    "httpsendrequest",
    "winhttpopenrequest",
    "wsasocket",
    // Unix / libc tells.
    "ptrace",
    "execve",
    "execvp",
    "execl",
    "fork",
    "vfork",
    "dlopen",
    "dlsym",
    "system",
    "popen",
    "mprotect",
    "setuid",
    "setgid",
    "prctl",
];

/// Whether `name` is a high-signal API. Case-insensitive and tolerant of the
/// Win32 `A`/`W` ANSI/Unicode suffix (`CreateProcessW` → `createprocess`).
pub fn is_suspicious(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stripped = lower
        .strip_suffix('a')
        .or_else(|| lower.strip_suffix('w'))
        .unwrap_or(&lower);
    SUSPICIOUS.contains(&lower.as_str()) || SUSPICIOUS.contains(&stripped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_known_apis_case_and_suffix_insensitive() {
        assert!(is_suspicious("VirtualAlloc"));
        assert!(is_suspicious("WriteProcessMemory"));
        assert!(is_suspicious("CreateProcessW")); // W suffix stripped
        assert!(is_suspicious("LoadLibraryA")); // A suffix stripped
        assert!(is_suspicious("ptrace")); // unix
        assert!(is_suspicious("GETPROCADDRESS")); // case-insensitive
    }

    #[test]
    fn ignores_benign_apis() {
        assert!(!is_suspicious("GetSystemTime"));
        assert!(!is_suspicious("printf"));
        assert!(!is_suspicious("WriteFile"));
        assert!(!is_suspicious(""));
    }
}
