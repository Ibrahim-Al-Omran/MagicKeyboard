; Custom NSIS hooks for Magic Keyboard.
; Registers the app to launch at Windows sign-in after install.

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Magic Keyboard" '"$INSTDIR\${MAINBINARYNAME}.exe" --background'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Magic Keyboard"
!macroend
