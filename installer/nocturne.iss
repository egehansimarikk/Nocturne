#ifndef AppVersion
  #define AppVersion "0.2.1"
#endif

[Setup]
AppId={{8E4F3A21-7C9B-4D1E-9A62-3F8B1C5D4E90}
AppName=Nocturne
AppVersion={#AppVersion}
AppVerName=Nocturne {#AppVersion}
AppPublisher=egehansimarikk
AppPublisherURL=https://github.com/egehansimarikk/nocturne
AppSupportURL=https://github.com/egehansimarikk/nocturne/issues
AppUpdatesURL=https://github.com/egehansimarikk/nocturne/releases
DefaultDirName={autopf}\Nocturne
DefaultGroupName=Nocturne
UninstallDisplayIcon={app}\nocturne.exe
SetupIconFile=..\assets\nocturne.ico
OutputDir=..\dist
OutputBaseFilename=Nocturne-Setup-v{#AppVersion}-win64
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
CloseApplications=yes
ShowLanguageDialog=auto
LanguageDetectionMethod=uilanguage

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "portuguese"; MessagesFile: "compiler:Languages\Portuguese.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "ukrainian"; MessagesFile: "compiler:Languages\Ukrainian.isl"
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "dutch"; MessagesFile: "compiler:Languages\Dutch.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "arabic"; MessagesFile: "compiler:Languages\Arabic.isl"
Name: "swedish"; MessagesFile: "compiler:Languages\Swedish.isl"
Name: "norwegian"; MessagesFile: "compiler:Languages\Norwegian.isl"
Name: "danish"; MessagesFile: "compiler:Languages\Danish.isl"
Name: "finnish"; MessagesFile: "compiler:Languages\Finnish.isl"
Name: "czech"; MessagesFile: "compiler:Languages\Czech.isl"
Name: "hungarian"; MessagesFile: "compiler:Languages\Hungarian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[InstallDelete]
Type: filesandordirs; Name: "{app}\*"

[Files]
Source: "..\target\release\nocturne.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\*.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\assets\nocturne.ico"; DestDir: "{app}\assets"; Flags: ignoreversion

[Icons]
Name: "{group}\Nocturne"; Filename: "{app}\nocturne.exe"; IconFilename: "{app}\assets\nocturne.ico"
Name: "{autodesktop}\Nocturne"; Filename: "{app}\nocturne.exe"; IconFilename: "{app}\assets\nocturne.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\nocturne.exe"; Description: "{cm:LaunchProgram,Nocturne}"; Flags: nowait postinstall skipifsilent