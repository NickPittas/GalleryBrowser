; Inno Setup script for GalleryBrowser (per-user install, no admin rights needed).
; Compile from the repo root:
;   ISCC /DAppVersion=0.2.0 /DSourceDir=<dir with gallerybrowser.exe, gallerybrowser.ico, NOTICE, README.md> ^
;        /DOutputDir=<dir> packaging\windows\installer.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist\GalleryBrowser"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif

#define AppName "GalleryBrowser"
#define AppExe "gallerybrowser.exe"

[Setup]
AppId={{6B1F3C52-9A0E-4E1D-8C55-4F2A7D0B9E31}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Nick Pittas
AppPublisherURL=https://github.com/NickPittas/GalleryBrowser
AppSupportURL=https://github.com/NickPittas/GalleryBrowser/issues
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=GalleryBrowser-{#AppVersion}-windows-x64-setup
SetupIconFile=gallerybrowser.ico
UninstallDisplayIcon={app}\gallerybrowser.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\gallerybrowser.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\NOTICE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"; IconFilename: "{app}\gallerybrowser.ico"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; IconFilename: "{app}\gallerybrowser.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
