; Before installing: an earlier Bobix built with Electron (electron-builder, per user, its own uninstall key) is
; uninstalled quietly, so there is one Bobix, not two. Its data (settings, pet pictures) and the hooks stay: they live
; outside its folder and this app reads the same places.
!macro NSIS_HOOK_PREINSTALL
  StrCpy $R0 0
  bobix_old_next:
    EnumRegKey $R1 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall" $R0
    StrCmp $R1 "" bobix_old_done
    IntOp $R0 $R0 + 1
    StrCmp $R1 "Bobix" bobix_old_next ; this app's own key
    ReadRegStr $R2 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$R1" "DisplayName"
    StrCpy $R3 $R2 5
    StrCmp $R3 "Bobix" 0 bobix_old_next
    ReadRegStr $R4 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$R1" "QuietUninstallString"
    StrCmp $R4 "" 0 +3
    ReadRegStr $R4 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$R1" "UninstallString"
    StrCpy $R4 "$R4 /S"
    DetailPrint "Removing $R2 (Electron build)"
    nsExec::Exec 'taskkill /F /IM Bobix.exe'
    Pop $R5
    ExecWait $R4
    ; the uninstaller may hand over to a copy of itself: wait (up to 20 s) until its key is gone
    StrCpy $R5 0
    bobix_old_wait:
      ReadRegStr $R2 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$R1" "DisplayName"
      StrCmp $R2 "" bobix_old_done
      IntOp $R5 $R5 + 1
      IntCmp $R5 40 bobix_old_done
      Sleep 500
      Goto bobix_old_wait
  bobix_old_done:
!macroend
