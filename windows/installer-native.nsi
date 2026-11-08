; Radiance Music for Windows, the native app (WinUI 3, issue #45, stage 5: the switch). It is today's Radiance Music's
; installer for its next version: the same folder, Start-menu entry and uninstall entry, so today's app updates into it
; (its update runs this silently into its own folder, then opens RadianceMusic.exe: 3ffd212, in 0.7.21). It takes today's
; app's program folders away first (as every update of it did), never your data: your songs, settings and the
; splitter it downloaded are in your data folders (%LOCALAPPDATA%\radiance-music, Music\Radiance Music), not here.
; makensis -DVERSION=x -DDIST=folder -DOUT=file -DICON=radiance-music.ico installer-native.nsi
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
!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\RadianceMusic.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Radiance Music"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Section "Radiance Music"
  ; the app's own files only: today's app's folders (the GTK app) and an older native app's, as every update did
  RMDir /r "$INSTDIR\bin"
  RMDir /r "$INSTDIR\lib"
  RMDir /r "$INSTDIR\share"
  RMDir /r "$INSTDIR\libexec"
  RMDir /r "$INSTDIR\etc"
  ; (test builds held the splitter here once; it's in the data folder now)
  RMDir /r "$INSTDIR\python"
  RMDir /r "$INSTDIR\models"
  ; an older native app's files at the top (its .NET and WinUI files: the dist's own list, written below, says which)
  IfFileExists "$INSTDIR\installed-files.txt" 0 install
    FileOpen $0 "$INSTDIR\installed-files.txt" r
    old:
      FileRead $0 $1
      StrCmp $1 "" done
      Push $1
      Call TrimNewline
      Pop $1
      Delete "$INSTDIR\$1"
      Goto old
    done:
    FileClose $0
  install:
  SetOutPath "$INSTDIR"
  File /r "${DIST}\*"
  File "/oname=radiance-music.ico" "${ICON}"
  CreateShortcut "$SMPROGRAMS\Radiance Music.lnk" "$INSTDIR\RadianceMusic.exe" "" "$INSTDIR\radiance-music.ico"
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
  ; radiancemusic:// links (a Radiance Music Server's "Open in Radiance Music") open the native app now
  WriteRegStr HKCU "Software\Classes\radiancemusic" "" "URL:Radiance Music"
  WriteRegStr HKCU "Software\Classes\radiancemusic" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\radiancemusic\DefaultIcon" "" "$INSTDIR\radiance-music.ico"
  WriteRegStr HKCU "Software\Classes\radiancemusic\shell\open\command" "" '"$INSTDIR\RadianceMusic.exe" "%1"'
SectionEnd
Function TrimNewline
  Exch $R0
  Push $R1
  loop:
    StrCpy $R1 $R0 1 -1
    StrCmp $R1 "$\r" trim
    StrCmp $R1 "$\n" trim
    Goto end
  trim:
    StrCpy $R0 $R0 -1
    Goto loop
  end:
  Pop $R1
  Exch $R0
FunctionEnd
Section "Uninstall"
  ; the app only: your liked songs, their parts and videos stay where they are
  Delete "$SMPROGRAMS\Radiance Music.lnk"
  RMDir /r "$INSTDIR"
  ; and the splitter it downloaded at its first start (1.3 GB; nothing of yours in it), as today's app's uninstaller
  RMDir /r "$LOCALAPPDATA\radiance-music\windows-splitter"
  DeleteRegKey HKCU "${KEY}"
  DeleteRegKey HKCU "Software\Classes\radiancemusic"
SectionEnd
