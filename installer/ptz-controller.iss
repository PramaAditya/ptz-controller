#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
AppId={{9B4E7C1F-8A2D-4C6B-9E31-5D8A0F2E4C1B}}
AppName=PTZ Controller
AppVersion={#AppVersion}
AppVerName=PTZ Controller {#AppVersion}
AppPublisher=PramaAditya
AppPublisherURL=https://github.com/PramaAditya/ptz-controller
AppSupportURL=https://github.com/PramaAditya/ptz-controller/issues
AppUpdatesURL=https://github.com/PramaAditya/ptz-controller/releases
DefaultDirName={autopf}\PTZ Controller
DefaultGroupName=PTZ Controller
AllowNoIcons=yes
LicenseFile=..\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=PTZ-Controller-Setup-v{#AppVersion}-windows-x64
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\ptz-controller.exe
UninstallDisplayName=PTZ Controller
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\ptz-controller.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\PTZ Controller"; Filename: "{app}\ptz-controller.exe"; IconFilename: "{app}\ptz-controller.exe"
Name: "{group}\Uninstall PTZ Controller"; Filename: "{uninstallexe}"
Name: "{autodesktop}\PTZ Controller"; Filename: "{app}\ptz-controller.exe"; IconFilename: "{app}\ptz-controller.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\ptz-controller.exe"; Description: "Launch PTZ Controller"; Flags: nowait postinstall skipifsilent
