; NewsQueue per-user installer (Inno Setup 7). Built by build-installer.ps1:
;   ISCC /DAppVersion=1.0.5 /DDistDir=<dist\NewsQueue-1.0.5> /DOutputDir=<dist> newsqueue.iss
;
; Copies from 1.0.2 to 1.0.5 update themselves by starting the setup with
; environment variables only (NQ_SILENT, NQ_INSTALL_DIR, NQ_NO_DESKTOP,
; NQ_WAIT_PID, NQ_RELAUNCH) and no command line. The [Code] section answers
; the wizard from those, so those copies keep updating. 1.0.6+ pass
; /VERYSILENT /DIR= instead.

#ifndef AppVersion
  #define AppVersion "1.0.5"
#endif
#ifndef DistDir
  #define DistDir "..\dist\NewsQueue-" + AppVersion
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
AppId={{7A3E2C51-9B4D-4F0E-8C21-6D5B9E1A4F37}
AppName=NewsQueue
AppVersion={#AppVersion}
AppVerName=NewsQueue {#AppVersion}
AppPublisher=NewsQueue
DefaultDirName={autopf}\NewsQueue
DefaultGroupName=NewsQueue
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=NewsQueue-{#AppVersion}-Setup
SetupIconFile=..\assets\newsqueue.ico
UninstallDisplayIcon={app}\newsqueue.exe
UninstallDisplayName=NewsQueue {#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
; Settings, queue, history, logs and downloads beside the exe are not part of
; the package, so neither an update nor the uninstaller touches them.
Source: "{#DistDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\NewsQueue"; Filename: "{app}\newsqueue.exe"; WorkingDir: "{app}"
Name: "{group}\Uninstall NewsQueue"; Filename: "{uninstallexe}"
Name: "{userdesktop}\NewsQueue"; Filename: "{app}\newsqueue.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\newsqueue.exe"; Description: "Start NewsQueue"; Flags: nowait postinstall skipifsilent; Check: not EnvSilent
Filename: "{app}\newsqueue.exe"; Flags: nowait; Check: RelaunchWanted

[Code]
var
  // Started by a 1.0.2-1.0.5 copy: environment variables, no command line.
  EnvSilentRun: Boolean;

function EnvSilent: Boolean;
begin
  Result := EnvSilentRun;
end;

function RelaunchWanted: Boolean;
begin
  Result := (WizardSilent or EnvSilentRun) and (GetEnv('NQ_RELAUNCH') = '1');
end;

function InitializeSetup: Boolean;
var
  Code: Integer;
begin
  Result := True;
  EnvSilentRun := (GetEnv('NQ_SILENT') = '1') and not WizardSilent;
  // A self-update starts the setup and then exits: wait up to a minute.
  if GetEnv('NQ_WAIT_PID') <> '' then
    Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
      '-NoProfile -Command "Wait-Process -Id ' + GetEnv('NQ_WAIT_PID') + ' -Timeout 60 -ErrorAction SilentlyContinue"',
      '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

// The running setup cannot start a silent copy of itself (Windows denies
// it access to its own file), so the env-var run fills in the wizard and
// clicks through it: folder and tasks preset, every page skipped.
procedure InitializeWizard;
begin
  if EnvSilentRun then
  begin
    if GetEnv('NQ_INSTALL_DIR') <> '' then
      WizardForm.DirEdit.Text := GetEnv('NQ_INSTALL_DIR');
  end;
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := EnvSilentRun;
  // The task list is filled by now; untick the desktop shortcut here.
  if EnvSilentRun and (PageID = wpSelectTasks) and (GetEnv('NQ_NO_DESKTOP') = '1') then
    WizardSelectTasks('!desktopicon');
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if EnvSilentRun and (CurPageID = wpFinished) then
    WizardForm.NextButton.OnClick(WizardForm.NextButton);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  // Replace an IExpress install (1.0.0 - 1.0.5): its uninstall entry, its
  // PowerShell uninstaller and its Start Menu uninstall shortcut. Done before
  // the files and shortcuts are written, so this setup's own survive.
  if CurStep = ssInstall then
  begin
    RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\NewsQueue');
    DeleteFile(ExpandConstant('{app}\uninstall.ps1'));
    DeleteFile(ExpandConstant('{userprograms}\NewsQueue\Uninstall NewsQueue.lnk'));
  end;
end;
