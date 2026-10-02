#ifndef SourceDir
  #error SourceDir must point at the validated Windows bundle
#endif
#ifndef OutputDir
  #error OutputDir must point at the installer output directory
#endif
#ifndef AppVersion
  #error AppVersion must match the Cargo workspace version
#endif

[Setup]
AppId={{18EA5C90-C00D-4ACD-BB47-3F396527E09D}
AppName=oca
AppVersion={#AppVersion}
AppPublisher=oca
AppPublisherURL=https://github.com/lucasgmagalhaes/oca
AppSupportURL=https://github.com/lucasgmagalhaes/oca/issues
AppUpdatesURL=https://github.com/lucasgmagalhaes/oca/releases
DefaultDirName={autopf}\oca
DefaultGroupName=oca
AllowNoIcons=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog
CloseApplications=yes
RestartApplications=no
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
LicenseFile={#SourceDir}\resources\licenses\oca.txt
UninstallDisplayIcon={app}\oca-desktop.exe
OutputDir={#OutputDir}
OutputBaseFilename=oca-x86_64-pc-windows-msvc-setup
VersionInfoVersion={#AppVersion}
VersionInfoDescription=oca video editor installer
VersionInfoProductName=oca
VersionInfoProductVersion={#AppVersion}
ShowLanguageDialog=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ptbr"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[CustomMessages]
en.DesktopIcon=Create a desktop shortcut
en.AdditionalShortcuts=Additional shortcuts:
en.LaunchOca=Launch oca
ptbr.DesktopIcon=Criar um atalho na área de trabalho
ptbr.AdditionalShortcuts=Atalhos adicionais:
ptbr.LaunchOca=Iniciar o oca

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; GroupDescription: "{cm:AdditionalShortcuts}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\oca"; Filename: "{app}\oca-desktop.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\oca"; Filename: "{app}\oca-desktop.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\oca-desktop.exe"; Description: "{cm:LaunchOca}"; Flags: nowait postinstall skipifsilent

[Code]
// oca-desktop.exe is a Tauri/WebView2 app. WebView2 Runtime ships preinstalled on Windows 11
// and reaches Windows 10 22H2 via Windows/Edge update, so no bootstrapper is bundled here (see
// packaging/bundle-manifest.json's system_contract) -- this only warns when it's still missing
// rather than silently fetching/running an unpinned Microsoft installer mid-setup.
function IsWebView2RuntimeInstalled(): Boolean;
var
  Version: String;
begin
  Result :=
    RegQueryStringValue(HKLM64, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version) or
    RegQueryStringValue(HKCU, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and not IsWebView2RuntimeInstalled() then
    MsgBox('oca precisa do Microsoft Edge WebView2 Runtime, que nao foi detectado neste ' +
      'computador. O Windows Update normalmente o instala automaticamente; caso oca nao ' +
      'inicie, baixe-o em https://developer.microsoft.com/microsoft-edge/webview2/.',
      mbInformation, MB_OK);
end;
