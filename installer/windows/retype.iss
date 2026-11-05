; Inno Setup script for Retype. Built by .github/workflows/release.yml:
;   iscc /DAppVersion=0.1.0 installer\windows\retype.iss
; Per-user install (no admin prompt) into %LOCALAPPDATA%\Programs\Retype.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6E0B1C42-5A7D-4F0E-9B1A-2C3D4E5F6071}
AppName=Retype
AppVersion={#AppVersion}
AppVerName=Retype {#AppVersion}
AppPublisher=Retype
AppPublisherURL=https://github.com/bunkhq/retype
AppSupportURL=https://github.com/bunkhq/retype/issues
AppUpdatesURL=https://github.com/bunkhq/retype/releases
DefaultDirName={localappdata}\Programs\Retype
DisableDirPage=auto
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\..\dist
OutputBaseFilename=Retype-{#AppVersion}-setup
SetupIconFile=..\..\assets\icons\retype.ico
UninstallDisplayIcon={app}\retype.exe
UninstallDisplayName=Retype
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Updates run silently; close the running Retype so its files can be replaced.
CloseApplications=force
RestartApplications=no

[Tasks]
Name: "contextmenu"; Description: "Add Retype to the Explorer right-click menu"
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\..\target\release\retype.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\windows\ffmpeg\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\..\THIRD-PARTY-NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Retype"; Filename: "{app}\retype.exe"
Name: "{autodesktop}\Retype"; Filename: "{app}\retype.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\retype.exe"; Parameters: "menu install"; Tasks: contextmenu; Flags: runhidden
; Also runs after a silent update, so Retype comes back on its own.
Filename: "{app}\retype.exe"; Description: "Open Retype"; Flags: nowait postinstall

[UninstallRun]
Filename: "{app}\retype.exe"; Parameters: "menu uninstall"; Flags: runhidden; RunOnceId: "RemoveMenu"
