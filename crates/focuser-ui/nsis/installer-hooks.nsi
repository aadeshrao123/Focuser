; Retired: strict Windows packages use the complete installer.nsi template.
; Do not attach these hooks to Tauri's stock template. It contains independent
; update/reinstall and cleanup paths which are outside our removal transaction.
!error "Use nsis/installer.nsi, pinned CLI 2.10.1, perMachine NSIS only"
