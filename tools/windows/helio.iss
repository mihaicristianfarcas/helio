; Helio's installer for Windows (Inno Setup 6.6 or newer), compiled by
; tools\package-windows.ps1 from the folder it stages:
;
;   ISCC /DVersion=X.Y.Z /DSource=STAGED_DIR /DOutput=OUT_DIR [/DSign /Sssign=COMMAND] tools\windows\helio.iss
;     -> OUT_DIR\Helio-X.Y.Z-windows-x86_64-setup.exe
;
; Per user, with no administrator (PrivilegesRequired=lowest), as Helio's
; data is: Helio.exe, the host's engine (helio-host.exe, the source copy
; `helio host enable` checks and copies into C:\Program Files\Helio Host),
; the `helio` command in bin (not on the PATH: Settings > General puts it
; there, helio-app/src/command.rs), FFmpeg's DLLs and FFmpeg's license, in
; %LOCALAPPDATA%\Programs\Helio; Helio in the Start menu and in Settings >
; Apps, under the key dev.mihaicristian.helio_is1 in HKCU's Uninstall.
;
; Hosting stays off: Helio's switch turns it on (Windows asks for an
; administrator then). Installed over a Helio whose hosting is on, the
; installer turns it on again from the new files, as Helio's own update
; does (helio-update/src/install/windows.rs): Windows asks, and the service
; runs the new engine. A per-user install cannot add firewall rules: Windows
; asks once, the first time Helio listens, as it does for any program; the
; host's rules come with `helio host enable`.
;
; Uninstalling turns hosting off first, while it is on (`helio-host
; disable`: the service, its firewall rules and the engine's copy in
; Program Files go; Windows asks for an administrator, and a refusal stops
; the uninstall with Helio in place). It also takes away what Helio made
; beside its files: `bin` on the user's PATH, Helio's start at login, and
; the files an in-app update brings that this installer did not (FFmpeg's
; DLLs of another version, files renamed aside as *.old). It keeps Helio's
; hosts and settings (%APPDATA%\Helio), the host's identity and pairings
; (C:\ProgramData\Helio Host), and the firewall rules Windows made when it
; asked about Helio.exe: only an administrator can remove those, and they
; let nothing in once the program is gone.
;
; /DSign signs Setup, the uninstaller and the uninstaller's copies of
; itself with the sign tool named ssign (tools\package-windows.ps1 -Sign
; defines it: Certum's cloud certificate, through ssign). The programs are
; signed before they get here.

#if Ver < EncodeVer(6, 6, 0)
  #error Inno Setup 6.6 or newer is needed (WizardStyle's dynamic mode)
#endif
#ifndef Version
  #error Pass /DVersion=X.Y.Z, the workspace's version
#endif
#ifndef Source
  #error Pass /DSource=DIR, the folder tools\package-windows.ps1 stages
#endif
#ifndef Output
  #define Output "."
#endif
; A debug build's programs are hundreds of megabytes: CI packages them
; with /DCompression=none rather than wait for LZMA.
#ifndef Compression
  #define Compression "lzma2/ultra64"
#endif

[Setup]
; The name of Helio's entry in Settings > Apps (Inno adds _is1), as the Mac
; app's bundle identifier is: no other program's entry has it. It must
; never change, or a new release installs beside the old one.
AppId=dev.mihaicristian.helio
AppName=Helio
AppVersion={#Version}
AppPublisher=Helio
AppPublisherURL=https://helio.mihaicristian.dev
AppSupportURL=https://github.com/mihaicristianfarcas/helio/issues
AppUpdatesURL=https://helio.mihaicristian.dev
VersionInfoVersion={#Version}
VersionInfoProductName=Helio
VersionInfoDescription=Helio Setup
UninstallDisplayName=Helio
UninstallDisplayIcon={app}\Helio.exe
PrivilegesRequired=lowest
; {userpf} is %LOCALAPPDATA%\Programs, per user.
DefaultDirName={userpf}\Helio
DisableDirPage=yes
DisableProgramGroupPage=yes
AlwaysShowDirOnReadyPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
; A Helio that runs holds its files: the installer closes it (Restart
; Manager), and asks first unless it runs silently.
CloseApplications=force
RestartApplications=no
WizardStyle=modern dynamic
SetupIconFile=..\..\helio-app\assets\Helio.ico
OutputDir={#Output}
OutputBaseFilename=Helio-{#Version}-windows-x86_64-setup
Compression={#Compression}
SolidCompression=yes
#ifdef Sign
SignTool=ssign
SignedUninstaller=yes
#endif

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#Source}\Helio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\helio-host.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\bin\helio.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
; What Helio.exe loads: avcodec, avutil, and swresample (some decoders).
Source: "{#Source}\avcodec-*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\avutil-*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\swresample-*.dll"; DestDir: "{app}"; Flags: ignoreversion
; FFmpeg is LGPL: its license goes with its DLLs.
Source: "{#Source}\FFmpeg LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
; What an older release left: FFmpeg's DLLs of another version (this
; release's are copied right after), and files its update renamed aside.
Type: files; Name: "{app}\avcodec-*.dll"
Type: files; Name: "{app}\avutil-*.dll"
Type: files; Name: "{app}\swresample-*.dll"
Type: files; Name: "{app}\*.old"
Type: files; Name: "{app}\bin\*.old"

[UninstallDelete]
; What an in-app update put in the folder, which the uninstaller's record
; of this install does not list. Only Helio's own names: the folder goes
; once it is empty, and only then.
Type: files; Name: "{app}\avcodec-*.dll"
Type: files; Name: "{app}\avutil-*.dll"
Type: files; Name: "{app}\swresample-*.dll"
Type: files; Name: "{app}\*.old"
Type: files; Name: "{app}\bin\*.old"
Type: dirifempty; Name: "{app}\bin"
Type: dirifempty; Name: "{app}"

[Icons]
Name: "{userprograms}\Helio"; Filename: "{app}\Helio.exe"; WorkingDir: "{app}"

[Run]
Filename: "{app}\Helio.exe"; WorkingDir: "{app}"; Description: "Open Helio"; Flags: nowait postinstall skipifsilent

[Code]
const
  { HelioHost, the service hosting runs as: registered while hosting is on
    (helio-host/src/service.rs). Anyone may read the services' keys. }
  HostService = 'SYSTEM\CurrentControlSet\Services\HelioHost';
  { Where Settings > General puts the command's folder (helio-app/src/command.rs). }
  UserEnvironment = 'Environment';
  { Where Settings > General makes Helio start at login, under the name
    Helio (helio-ui/src/login.rs, helio-app/src/settings.rs). }
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  { What Windows says when the user answers No to UAC. }
  ERROR_CANCELLED = 1223;
  { HWND_BROADCAST is Inno's own. }
  WM_SETTINGCHANGE = $001A;
  SMTO_ABORTIFHUNG = $0002;

function ExpandEnvironmentStrings(Source: String; Dest: String; Size: Cardinal): Cardinal;
  external 'ExpandEnvironmentStringsW@kernel32.dll stdcall';
function SendMessageTimeout(Window: HWND; Msg: Cardinal; WParam: Cardinal; LParam: String;
  Flags: Cardinal; Timeout: Cardinal; var Answer: Cardinal): Cardinal;
  external 'SendMessageTimeoutW@user32.dll stdcall';

function HostingIsOn: Boolean;
begin
  Result := RegKeyExists(HKLM, HostService);
end;

function PowerShell: String;
begin
  Result := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');
end;

{ S as a PowerShell string in single quotes. }
function PsQuoted(S: String): String;
begin
  StringChangeEx(S, '''', '''''', True);
  Result := '''' + S + '''';
end;

{ `helio-host Verb` as an administrator, with the engine installed here
  (else the service's own copy): Windows asks. Its exit code, or
  ERROR_CANCELLED when it did not start (declined, as a rule). }
function RunEngineElevated(Verb: String): Integer;
var
  Engine, Script: String;
begin
  Engine := ExpandConstant('{app}\helio-host.exe');
  if not FileExists(Engine) then
    Engine := ExpandConstant('{commonpf64}\Helio Host\helio-host.exe');
  Script := '$ErrorActionPreference = ''Stop''; ' +
    'try { $p = Start-Process -FilePath ' + PsQuoted(Engine) + ' -ArgumentList ' + Verb +
    ' -Verb RunAs -WindowStyle Hidden -Wait -PassThru } catch { exit ' + IntToStr(ERROR_CANCELLED) + ' }; ' +
    'exit $p.ExitCode';
  if not Exec(PowerShell, '-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "' + Script + '"',
      '', SW_HIDE, ewWaitUntilTerminated, Result) then
    Result := ERROR_CANCELLED;
end;

{ Installed over a Helio whose hosting is on: the host is put on the new
  engine, as Helio's own update does. Declined, Helio is installed all the
  same and the host keeps the engine it had. }
procedure CurStepChanged(CurStep: TSetupStep);
var
  Code: Integer;
begin
  if (CurStep <> ssPostInstall) or not HostingIsOn then
    Exit;
  Code := RunEngineElevated('enable');
  if Code <> 0 then
    SuppressibleMsgBox('Helio is installed, but its host still runs the engine it had: ' +
      'Windows was not allowed to update it (helio-host enable, exit code ' + IntToStr(Code) + '). ' +
      'Turn hosting off and on in Helio to bring it up to date.', mbInformation, MB_OK, IDOK);
end;

{ Helio, and the `helio` command, running from this folder: the files they
  hold cannot go. Another copy (a download unpacked elsewhere, a build) is
  left alone: the path tells them apart. }
procedure StopHelio;
var
  Code: Integer;
begin
  Exec(PowerShell, '-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "' +
    '$d = ' + PsQuoted(ExpandConstant('{app}') + '\') + '; ' +
    '$p = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and ' +
    '$_.Path.StartsWith($d, [StringComparison]::OrdinalIgnoreCase) }); ' +
    '$p | Stop-Process -Force -ErrorAction SilentlyContinue; ' +
    '$p | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue"',
    '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

{ %NAME%s in S replaced, as Windows expands a REG_EXPAND_SZ. }
function Expanded(S: String): String;
var
  Size: Cardinal;
begin
  Size := ExpandEnvironmentStrings(S, '', 0);
  if Size = 0 then begin
    Result := S;
    Exit;
  end;
  SetLength(Result, Size);
  ExpandEnvironmentStrings(S, Result, Size);
  SetLength(Result, Size - 1);
end;

function WithoutTrailingSeparator(S: String): String;
begin
  while (Length(S) > 0) and (S[Length(S)] = '\') do
    SetLength(S, Length(S) - 1);
  Result := S;
end;

{ Two folders are one when they differ only in case or a trailing
  separator, as Windows compares them (helio-app/src/command.rs). }
function SameFolder(A, B: String): Boolean;
begin
  Result := CompareText(WithoutTrailingSeparator(A), WithoutTrailingSeparator(B)) = 0;
end;

{ The command's folder off the user's PATH, where Settings > General put
  it: every entry that names it, however written, and nothing else. Written
  back as the REG_EXPAND_SZ Windows keeps it as, then announced, so that
  what Explorer starts from now on has it. }
procedure RemoveCommandFromPath;
var
  Path, Rest, Entry, Bin: String;
  Kept: String;
  Found: Boolean;
  Split: Integer;
  Answer: Cardinal;
begin
  if not RegQueryStringValue(HKCU, UserEnvironment, 'Path', Path) then
    Exit;
  Bin := ExpandConstant('{app}\bin');
  Kept := '';
  Found := False;
  Rest := Path + ';';
  while Rest <> '' do begin
    Split := Pos(';', Rest);
    Entry := Copy(Rest, 1, Split - 1);
    Delete(Rest, 1, Split);
    if (Entry <> '') and SameFolder(Expanded(Entry), Bin) then
      Found := True
    else if Entry <> '' then begin
      if Kept <> '' then
        Kept := Kept + ';';
      Kept := Kept + Entry;
    end;
  end;
  if not Found then
    Exit;
  if Kept = '' then
    RegDeleteValue(HKCU, UserEnvironment, 'Path')
  else
    RegWriteExpandStringValue(HKCU, UserEnvironment, 'Path', Kept);
  SendMessageTimeout(HWND_BROADCAST, WM_SETTINGCHANGE, 0, 'Environment', SMTO_ABORTIFHUNG, 5000, Answer);
end;

{ Helio's start at login, when it starts this copy. }
procedure RemoveStartAtLogin;
var
  Command: String;
begin
  if RegQueryStringValue(HKCU, RunKey, 'Helio', Command) and
      (Pos(Lowercase('"' + ExpandConstant('{app}\Helio.exe') + '"'), Lowercase(Command)) = 1) then
    RegDeleteValue(HKCU, RunKey, 'Helio');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Code: Integer;
begin
  case CurUninstallStep of
    usUninstall:
      begin
        { Hosting off first: a refusal or a failure stops the uninstall
          here, with Helio in place to turn hosting off from. }
        if HostingIsOn then begin
          Code := RunEngineElevated('disable');
          if Code = ERROR_CANCELLED then begin
            SuppressibleMsgBox('Helio is still installed. Hosting is on, and only an administrator can ' +
              'turn it off: uninstall again and choose Yes when Windows asks, or turn hosting off in Helio ' +
              'first.', mbError, MB_OK, IDOK);
            Abort;
          end else if Code <> 0 then begin
            SuppressibleMsgBox('Helio is still installed. Hosting could not be turned off ' +
              '(helio-host disable, exit code ' + IntToStr(Code) + '): turn hosting off in Helio, then ' +
              'uninstall again.', mbError, MB_OK, IDOK);
            Abort;
          end;
        end;
        StopHelio;
      end;
    usPostUninstall:
      begin
        RemoveCommandFromPath;
        RemoveStartAtLogin;
      end;
  end;
end;
