#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef BinDir
  #define BinDir "..\target\x86_64-pc-windows-gnu\release"
#endif

[Setup]
AppId={{1191DD23-A70B-4FFB-B77D-37FE0F1A8601}
AppName=Optimum
AppVersion={#AppVersion}
AppVerName=Optimum {#AppVersion}
AppPublisher=Optimum
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\Optimum
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.18362
LicenseFile=..\LICENSE
SetupIconFile=..\assets\optimum.ico
UninstallDisplayIcon={app}\optimum.exe
UninstallDisplayName=Optimum
ChangesAssociations=yes
WizardStyle=modern
WizardImageFile=wizard-100.bmp,wizard-125.bmp,wizard-150.bmp,wizard-200.bmp
WizardSmallImageFile=wizard-small-100.bmp,wizard-small-125.bmp,wizard-small-150.bmp,wizard-small-200.bmp
CloseApplications=yes
RestartApplications=no
Compression=lzma2/max
SolidCompression=yes
OutputDir=..\target\installer
OutputBaseFilename=optimum-setup-{#AppVersion}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "ukrainian"; MessagesFile: "compiler:Languages\Ukrainian.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#BinDir}\optimum.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BinDir}\conpty.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BinDir}\OpenConsole.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Optimum"; Filename: "{app}\optimum.exe"
Name: "{autodesktop}\Optimum"; Filename: "{app}\optimum.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\optimum.exe"; Description: "{cm:LaunchProgram,Optimum}"; Flags: nowait postinstall skipifsilent
