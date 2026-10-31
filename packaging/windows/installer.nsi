; Radiance Music for Windows: a per-user installer (no administrator needed), made from the
; folder packaging/windows/pack.sh puts together. makensis -DVERSION=x -DDIST=folder -DOUT=file
Unicode true
!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define NAME "Radiance Music"
!define KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\RadianceMusic"

Name "${NAME}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\Radiance Music"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "${NAME} ${VERSION}, an open beta"

!include "MUI2.nsh"
!define MUI_ICON "radiance-music.ico"
!define MUI_UNICON "radiance-music.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\bin\radiance-music.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Radiance Music"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Radiance Music"
  ; an update replaces the app's own files (your songs live in Music\Radiance Music, not here)
  RMDir /r "$INSTDIR\bin"
  RMDir /r "$INSTDIR\lib"
  RMDir /r "$INSTDIR\share"
  RMDir /r "$INSTDIR\libexec"
  ; (test builds held the splitter here; it's downloaded into the data folder at the first start now)
  RMDir /r "$INSTDIR\python"
  RMDir /r "$INSTDIR\models"
  SetOutPath "$INSTDIR"
  File /r "${DIST}\*"
  File "radiance-music.ico"
  CreateShortcut "$SMPROGRAMS\Radiance Music.lnk" "$INSTDIR\bin\radiance-music.exe" "" "$INSTDIR\radiance-music.ico"
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
  ; radiancemusic:// links (a Radiance Music Server's "Open in Radiance Music") open the app
  WriteRegStr HKCU "Software\Classes\radiancemusic" "" "URL:Radiance Music"
  WriteRegStr HKCU "Software\Classes\radiancemusic" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\radiancemusic\DefaultIcon" "" "$INSTDIR\radiance-music.ico"
  WriteRegStr HKCU "Software\Classes\radiancemusic\shell\open\command" "" '"$INSTDIR\bin\radiance-music.exe" "%1"'
SectionEnd

Section "Uninstall"
  ; the app only: your liked songs, their parts and videos stay where they are
  Delete "$SMPROGRAMS\Radiance Music.lnk"
  RMDir /r "$INSTDIR"
  ; and the splitter it downloaded at its first start (1.3 GB; nothing of yours in it)
  RMDir /r "$LOCALAPPDATA\radiance-music\windows-splitter"
  DeleteRegKey HKCU "${KEY}"
  DeleteRegKey HKCU "Software\Classes\radiancemusic"
SectionEnd
