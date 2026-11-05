; Radiance Music (beta), the native Windows app (issue #45): a per-user installer (no administrator),
; beside today's Radiance Music (its own folder, Start-menu entry and uninstall entry). It shares
; today's app's songs, settings and splitter (they're in your data folders, not here).
; radiancemusic:// links stay with today's app until the beta has servers (stage 4).
; makensis -DVERSION=x -DDIST=folder -DOUT=file -DICON=radiance-music.ico installer-beta.nsi
Unicode true
!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define NAME "Radiance Music (beta)"
!define KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\RadianceMusicBeta"
Name "${NAME}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\Radiance Music (beta)"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "${NAME} ${VERSION}"
!include "MUI2.nsh"
!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\RadianceMusicBeta.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Radiance Music (beta)"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Section "Radiance Music (beta)"
  ; an update replaces the app's own files
  RMDir /r "$INSTDIR"
  SetOutPath "$INSTDIR"
  File /r "${DIST}\*"
  CreateShortcut "$SMPROGRAMS\Radiance Music (beta).lnk" "$INSTDIR\RadianceMusicBeta.exe" "" "$INSTDIR\radiance-music.ico"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "${KEY}" "DisplayName" "${NAME}"
  WriteRegStr HKCU "${KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${KEY}" "Publisher" "Radiance Innovation Centers"
  WriteRegStr HKCU "${KEY}" "DisplayIcon" "$INSTDIR\radiance-music.ico"
  WriteRegStr HKCU "${KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${KEY}" "URLInfoAbout" "https://music.radianceph.com"
  WriteRegDWORD HKCU "${KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${KEY}" "NoRepair" 1
SectionEnd
Section "Uninstall"
  ; the beta only: your songs, settings and today's app's splitter stay where they are
  Delete "$SMPROGRAMS\Radiance Music (beta).lnk"
  RMDir /r "$INSTDIR"
  DeleteRegKey HKCU "${KEY}"
SectionEnd
