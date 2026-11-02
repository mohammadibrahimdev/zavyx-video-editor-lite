; Inno Setup script for Zavyx Video Editor.
; Built by scripts\package.ps1, which stages files into dist\staging-<arch> first:
;   iscc /DAppVersion=0.1.0 /DArch=x64 installer\zavyx-ve.iss
;
; Installs per-user (no admin prompt). The x64 installer only runs on 64-bit
; Windows; the x86 one runs everywhere.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#define AppName "Zavyx Video Editor"

[Setup]
AppId={{8E7A1B2C-3D4E-5F60-8190-2B3C4D5E6F70}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Mohammad Ibrahim
AppPublisherURL=https://github.com/mohammadibrahimdev
AppSupportURL=https://github.com/mohammadibrahimdev/zavyx-video-editor/issues
AppUpdatesURL=https://github.com/mohammadibrahimdev/zavyx-video-editor/releases
DefaultDirName={localappdata}\Programs\Zavyx VE
DefaultGroupName=Zavyx
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
#if Arch == "x64"
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
OutputDir=..\dist
OutputBaseFilename=ZavyxVE-Setup-{#AppVersion}-{#Arch}
SetupIconFile=..\assets\zavyx-ve.ico
UninstallDisplayIcon={app}\ZavyxVE.exe
UninstallDisplayName={#AppName}
LicenseFile=..\LICENSE-MIT
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
CloseApplications=force
ChangesAssociations=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "..\dist\staging-{#Arch}\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion

[Icons]
Name: "{group}\Zavyx VE"; Filename: "{app}\ZavyxVE.exe"
Name: "{group}\Uninstall Zavyx VE"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Zavyx VE"; Filename: "{app}\ZavyxVE.exe"; Tasks: desktopicon

[Registry]
; Double-clicking a .zvep project opens it in Zavyx VE.
Root: HKCU; Subkey: "Software\Classes\.zvep"; ValueType: string; ValueName: ""; ValueData: "ZavyxVE.Project"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\ZavyxVE.Project"; ValueType: string; ValueName: ""; ValueData: "Zavyx VE Project"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\ZavyxVE.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\ZavyxVE.exe,0"
Root: HKCU; Subkey: "Software\Classes\ZavyxVE.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\ZavyxVE.exe"" ""%1"""

[Run]
Filename: "{app}\ZavyxVE.exe"; Description: "Start Zavyx VE now"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM ZavyxVE.exe /F"; Flags: runhidden; RunOnceId: "StopZavyxVE"
