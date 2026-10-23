//! A Windows node reached in its own Windows PowerShell over an ssh login. Its drives are the physical
//! disks with their lettered volumes, each walked from its letter's root. Every path is followed link by
//! link — a symbolic link or a junction to its target — and refused where any step leaves the volume,
//! and every change is held to the settings' protected locations for a Windows machine.
//!
//! Every name reaches a script as base64 of its UTF-8, since PowerShell takes more characters than
//! the plain apostrophe as a quote and no name should be able to end a literal early.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::super::Span;
use super::super::drives::Drive;
use super::shell::{Op, Rooted, Shell, Source, WINDOWS_DRIVES, code, encoded_command, read_windows_drives};
use crate::settings::WindowsProtect;

/// Windows PowerShell, with the protected locations its scripts hold every change to.
pub struct PowerShell {
  pub policy: WindowsProtect,
}

// Characters Windows takes in no name.
const NOT_IN_A_NAME: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// A text as an expression that gives it back whole.
fn text(value: &str) -> String {
  format!("(T '{}')", STANDARD.encode(value))
}

fn texts(names: &[String]) -> String {
  format!("@({})", names.iter().map(|name| text(name)).collect::<Vec<_>>().join(", "))
}

fn literal(value: &str) -> String {
  format!("'{}'", value.replace('\'', "''"))
}

impl PowerShell {
  /// The functions every script stands on: the volume's real top, the fence, and the guard.
  fn prelude(&self, root: &str) -> String {
    let policy = &self.policy;
    let system = policy.system.iter().map(|one| literal(one)).collect::<Vec<_>>().join(", ");
    let home = policy.home.iter().map(|one| literal(one)).collect::<Vec<_>>().join(", ");
    let (missing, outside, denied, protected, not_folder, looped) = (
      code::MISSING,
      code::OUTSIDE,
      code::DENIED,
      code::PROTECTED,
      code::NOT_FOLDER,
      code::LOOPED,
    );

    format!(
      r#"$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
function T([string]$b) {{ [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($b)) }}
$top = [IO.Path]::GetFullPath({root})
function Inside([string]$p) {{
  $t = $top.TrimEnd('\')
  $p.Equals($t, [StringComparison]::OrdinalIgnoreCase) -or $p.StartsWith($t + '\', [StringComparison]::OrdinalIgnoreCase)
}}
function Item([string]$p) {{
  try {{ Get-Item -LiteralPath $p -Force }} catch {{
    if ($_.Exception -is [UnauthorizedAccessException] -or $_.CategoryInfo.Category -eq 'PermissionDenied') {{ exit {denied} }}
    exit {missing}
  }}
}}
function Follow([string]$p) {{
  for ($hop = 0; $hop -lt 40; $hop++) {{
    $i = Item $p
    if ($i.LinkType -ne 'SymbolicLink' -and $i.LinkType -ne 'Junction') {{
      if (-not (Inside $i.FullName)) {{ exit {outside} }}
      return $i
    }}
    $t = @($i.Target)[0]
    if (-not $t) {{ exit {missing} }}
    if (-not [IO.Path]::IsPathRooted($t)) {{ $t = [IO.Path]::Combine([IO.Path]::GetDirectoryName($p), $t) }}
    $p = [IO.Path]::GetFullPath($t)
    if (-not (Inside $p)) {{ exit {outside} }}
  }}
  exit {missing}
}}
function Holds([string]$p, [string]$w) {{
  $c = [StringComparison]::OrdinalIgnoreCase
  $p = $p.TrimEnd('\')
  $w = $w.TrimEnd('\')
  $w.Equals($p, $c) -or $w.StartsWith($p + '\', $c)
}}
function Reach([string[]]$names) {{
  $i = Follow $top
  $walked = @($i.FullName)
  foreach ($n in $names) {{
    $i = Follow ([IO.Path]::Combine($i.FullName, $n))
    foreach ($w in $walked) {{ if (Holds $i.FullName $w) {{ exit {looped} }} }}
    $walked += $i.FullName
  }}
  $i
}}
function Entry([string[]]$folder, [string]$name) {{
  $d = Reach $folder
  if (-not $d.PSIsContainer) {{ exit {not_folder} }}
  [IO.Path]::Combine($d.FullName, $name)
}}
function There([string]$p) {{ Test-Path -LiteralPath $p }}
function Near([string]$p, [string]$s) {{
  if (-not $s) {{ return }}
  $s = [IO.Path]::GetFullPath($s).TrimEnd('\')
  $q = $p.TrimEnd('\')
  $c = [StringComparison]::OrdinalIgnoreCase
  if ($q.Equals($s, $c) -or $q.StartsWith($s + '\', $c) -or $s.StartsWith($q + '\', $c)) {{ exit {protected} }}
}}
function Guard([string]$p) {{
  if ($p.TrimEnd('\').Equals([IO.Path]::GetPathRoot($p).TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) {{ exit {protected} }}
  foreach ($v in @({system})) {{ Near $p ([Environment]::GetEnvironmentVariable($v)) }}
  foreach ($h in @({home})) {{ Near $p (Join-Path $env:USERPROFILE $h) }}
}}
"#,
      root = text(root),
    )
  }
}

/// The folder's names and the entry's own name of a path.
fn split(names: &[String]) -> (&[String], &str) {
  match names.split_last() {
    Some((last, folder)) => (folder, last.as_str()),
    None => (&[], ""),
  }
}

fn entry(names: &[String]) -> String {
  let (folder, name) = split(names);

  format!("Entry {} {}", texts(folder), text(name))
}

/// The lines setting `$start` and `$left` to where the bytes a span names start in a file of `$size`
/// bytes and how many there are.
fn spanned(span: Span) -> String {
  match span {
    Span::From { start, most } => format!(
      "$start = [Math]::Min([long]{start}, $size)\n$left = {}",
      most.map_or_else(
        || "$size - $start".to_owned(),
        |most| format!("[Math]::Min([long]{most}, $size - $start)")
      )
    ),
    Span::Last(last) => format!("$start = [Math]::Max([long]0, $size - {last})\n$left = $size - $start"),
  }
}

impl Shell for PowerShell {
  fn drives_script(&self) -> String {
    WINDOWS_DRIVES.to_owned()
  }

  fn read_drives(&self, printed: &[u8]) -> Result<Vec<Drive<Rooted>>, String> {
    Ok(read_windows_drives(&String::from_utf8_lossy(printed), |letter| {
      Some(format!("{letter}\\"))
    }))
  }

  fn allows(&self, name: &str) -> bool {
    !name.chars().any(|one| one < ' ' || NOT_IN_A_NAME.contains(&one))
  }

  fn beneath(&self, root: &str, names: &[String]) -> String {
    if names.is_empty() {
      return root.to_owned();
    }

    format!("{}\\{}", root.trim_end_matches(['\\', '/']), names.join("\\"))
  }

  fn script(&self, root: &str, op: &Op) -> String {
    let body = match op {
      Op::List(names) => format!(
        r#"$d = Reach {}
if (-not $d.PSIsContainer) {{ exit {} }}
try {{ [void][IO.Directory]::EnumerateFileSystemEntries($d.FullName).GetEnumerator().MoveNext() }} catch {{ exit {} }}
$out = [Console]::OpenStandardOutput()
$epoch = New-Object DateTime 1970, 1, 1, 0, 0, 0, ([DateTimeKind]::Utc)
$all = @(Get-ChildItem -LiteralPath $d.FullName -Force -ErrorAction SilentlyContinue)
foreach ($e in $all) {{
  $k = if ($e.PSIsContainer) {{ 'd' }} elseif ($e -is [IO.FileInfo]) {{ 'f' }} else {{ 'o' }}
  $s = if ($k -eq 'f') {{ $e.Length }} else {{ 0 }}
  $m = ($e.LastWriteTimeUtc - $epoch).TotalSeconds.ToString([Globalization.CultureInfo]::InvariantCulture)
  $b = [Text.Encoding]::UTF8.GetBytes("$k`t$s`t$m`t$($e.Name)`0")
  $out.Write($b, 0, $b.Length)
}}
$out.WriteByte(0)
foreach ($e in $all) {{
  if ($e.LinkType -ne 'SymbolicLink' -and $e.LinkType -ne 'Junction') {{ continue }}
  $k = if ($e.PSIsContainer) {{ 'd' }} else {{ 'f' }}
  $b = [Text.Encoding]::UTF8.GetBytes("$($e.Name)`0$k`0$(@($e.Target)[0])`0")
  $out.Write($b, 0, $b.Length)
}}
$out.Flush()
"#,
        texts(names),
        code::NOT_FOLDER,
        code::DENIED
      ),
      Op::Read(names, span) => format!(
        r#"$f = Reach {}
if ($f.PSIsContainer) {{ exit {} }}
try {{ $in = [IO.File]::Open($f.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite) }} catch {{ exit {} }}
$size = $in.Length
{}
[void]$in.Seek($start, [IO.SeekOrigin]::Begin)
$out = [Console]::OpenStandardOutput()
$b = [Text.Encoding]::ASCII.GetBytes("$size`n")
$out.Write($b, 0, $b.Length)
$buf = New-Object byte[] 65536
while ($left -gt 0) {{
  $n = $in.Read($buf, 0, [int][Math]::Min($buf.Length, $left))
  if ($n -le 0) {{ break }}
  $out.Write($buf, 0, $n)
  $left -= $n
}}
$out.Flush()
$in.Close()
"#,
        texts(names),
        code::IS_FOLDER,
        code::DENIED,
        spanned(*span)
      ),
      Op::MakeFolder(names) => format!(
        r#"$e = {}
Guard $e
if (There $e) {{ exit {} }}
try {{ [void][IO.Directory]::CreateDirectory($e) }} catch {{ exit {} }}
"#,
        entry(names),
        code::TAKEN,
        code::DENIED
      ),
      Op::Write(names, source, length) => {
        let (scratch, open) = match source {
          Source::Stdin => ("$null".to_owned(), "[Console]::OpenStandardInput()"),
          Source::Scratch(path) => (
            format!("[IO.Path]::Combine($env:USERPROFILE, {})", text(path)),
            "[IO.File]::OpenRead($x)",
          ),
        };

        format!(
          r#"$e = {}
Guard $e
if (Test-Path -LiteralPath $e -PathType Container) {{ exit {} }}
$s = [IO.Path]::Combine([IO.Path]::GetDirectoryName($e), ".bmo-upload-$PID")
$x = {scratch}
$w = $null
try {{
  try {{ $w = [IO.File]::Open($s, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write) }} catch {{ exit {} }}
  $in = {open}
  $in.CopyTo($w)
  $in.Close()
  $w.Close()
  $w = $null
  if ((Get-Item -LiteralPath $s -Force).Length -ne {length}) {{ exit {} }}
  try {{
    if (Test-Path -LiteralPath $e) {{ [IO.File]::Delete($e) }}
    [IO.File]::Move($s, $e)
  }} catch {{ exit {} }}
}} finally {{
  if ($w) {{ $w.Dispose() }}
  if (Test-Path -LiteralPath $s) {{ Remove-Item -LiteralPath $s -Force }}
  if ($x -and (Test-Path -LiteralPath $x)) {{ Remove-Item -LiteralPath $x -Force }}
}}
"#,
          entry(names),
          code::IS_FOLDER,
          code::DENIED,
          code::CUT_SHORT,
          code::DENIED
        )
      }
      Op::Rename(names, name) => format!(
        r#"$e = {}
if (-not (There $e)) {{ exit {} }}
Guard $e
$t = [IO.Path]::Combine([IO.Path]::GetDirectoryName($e), {})
Guard $t
if (There $t) {{ exit {} }}
try {{ Move-Item -LiteralPath $e -Destination $t }} catch {{ exit {} }}
"#,
        entry(names),
        code::MISSING,
        text(name),
        code::TAKEN,
        code::DENIED
      ),
      Op::Remove(names) => format!(
        r#"$e = {}
if (-not (There $e)) {{ exit {} }}
Guard $e
$i = Get-Item -LiteralPath $e -Force
try {{
  if (-not $i.PSIsContainer) {{ [IO.File]::Delete($e) }}
  elseif ($i.LinkType) {{ [IO.Directory]::Delete($e, $false) }}
  else {{ [IO.Directory]::Delete($e, $true) }}
}} catch {{ exit {} }}
"#,
        entry(names),
        code::MISSING,
        code::DENIED
      ),
      Op::Copy { from, to, moving } => format!(
        r#"$e = {}
if (-not (There $e)) {{ exit {} }}
{}$t = {}
Guard $t
if (There $t) {{ exit {} }}
if ((Test-Path -LiteralPath $e -PathType Container) -and $t.StartsWith($e.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {{ exit {} }}
try {{ {} -LiteralPath $e -Destination $t }} catch {{ exit {} }}
"#,
        entry(from),
        code::MISSING,
        if *moving { "Guard $e\n" } else { "" },
        entry(to),
        code::TAKEN,
        code::INTO_ITSELF,
        if *moving { "Move-Item" } else { "Copy-Item -Recurse" },
        code::DENIED
      ),
    };

    format!("{}{body}exit 0\n", self.prelude(root))
  }

  /// The script run by the login's own PowerShell as a script block: a second PowerShell started under
  /// it would never see the end of the login's standard input, which an upload is.
  fn login_command(&self, script: &str) -> String {
    format!(
      "& ([ScriptBlock]::Create([Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('{}'))))",
      encoded_command(script)
    )
  }

  /// Windows PowerShell under an ssh login stops reading its standard input after its first few
  /// tens of kilobytes.
  fn reads_uploads(&self) -> bool {
    false
  }
}
