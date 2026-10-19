//! A node reached in a POSIX shell: a Linux node over an ssh login or a jake hub's exec, and a WSL
//! node, whose Windows drives are read through the Windows beneath it. Every path is followed with
//! `realpath` to where it really stands before anything is opened, and every change is held to the
//! settings' protected locations for such a machine, under the node's own home.

use std::borrow::Cow;

use super::super::Span;
use super::super::drives::{Drive, read_drives, windows_mounts};
use super::shell::{Op, Rooted, Shell, Source, WINDOWS_DRIVES, code, encoded_command, read_windows_drives};
use crate::settings::PosixProtect;

/// The POSIX shell, with the protected locations its scripts hold every change to.
pub struct Posix {
  pub policy: PosixProtect,
}

// Where WSL finds Windows PowerShell.
const WINDOWS_POWERSHELL: &str = "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe";

// The mounts WSL itself stands on, which are no drive of the machine's.
const WSL_OWN: &[&str] = &["/mnt/wsl", "/usr/lib/wsl", "/init"];

/// One word as the shell reads it back whole. Every word handed here was refused before it could
/// hold a NUL, the one character no shell word can carry.
fn quoted(word: &str) -> Cow<'_, str> {
  shlex::try_quote(word).unwrap_or(Cow::Borrowed("''"))
}

fn words(names: &[String]) -> String {
  names.iter().map(|name| quoted(name).into_owned()).collect::<Vec<_>>().join(" ")
}

impl Posix {
  /// The functions every script stands on: the volume's real top, the fence, and the guard.
  fn prelude(&self, root: &str) -> String {
    let policy = &self.policy;
    let tops = policy.top.iter().map(|one| quoted(one).into_owned()).collect::<Vec<_>>().join(" ");
    let within = policy
      .system
      .iter()
      .map(|one| quoted(one).into_owned())
      .chain(policy.home.iter().map(|one| format!("\"$HOME\"/{}", quoted(one))))
      .collect::<Vec<_>>()
      .join(" ");
    let named = policy
      .home_named
      .iter()
      .map(|one| {
        format!(
          "for s in \"$HOME\"/{}/{}*; do [ -e \"$s\" ] && near \"$1\" \"$s\"; done\n",
          quoted(&one.under),
          quoted(&one.starting)
        )
      })
      .collect::<String>();
    let (missing, outside, protected) = (code::MISSING, code::OUTSIDE, code::PROTECTED);

    format!(
      r#"top=$(realpath -e -- {root}) || exit {missing}
inside() {{ case "${{1%/}}/" in "${{top%/}}/"*) return 0 ;; esac; return 1; }}
reach() {{
  p=$top
  for n in "$@"; do p=${{p%/}}/$n; done
  r=$(realpath -e -- "$p") || exit {missing}
  inside "$r" || exit {outside}
  printf '%s\n' "$r"
}}
there() {{ [ -e "$1" ] || [ -L "$1" ]; }}
near() {{
  s=$(realpath -m -- "$2")
  case "${{1%/}}/" in "${{s%/}}/"*) exit {protected} ;; esac
  case "${{s%/}}/" in "${{1%/}}/"*) exit {protected} ;; esac
  return 0
}}
guard() {{
  [ "$1" = / ] && exit {protected}
  for s in {tops}; do [ "$(dirname -- "$1")" = "$(realpath -m -- "$s")" ] && exit {protected}; done
  for s in {within}; do near "$1" "$s"; done
  {named}  return 0
}}
"#,
      root = quoted(root),
    )
  }
}

/// A path split into its folder's names and its own name.
fn split(names: &[String]) -> (&[String], &str) {
  match names.split_last() {
    Some((last, folder)) => (folder, last.as_str()),
    None => (&[], ""),
  }
}

/// The command streaming the bytes a span names of the file at `$f`, from where they start: `tail`
/// seeks a regular file rather than reading up to it.
fn streamed(span: Span) -> String {
  match span {
    Span::From {
      start: 0,
      most: None,
    } => "exec cat -- \"$f\"".to_owned(),
    Span::From {
      start: 0,
      most: Some(most),
    } => format!("exec head -c {most} -- \"$f\""),
    Span::From { start, most: None } => format!("exec tail -c +{} -- \"$f\"", start + 1),
    Span::From {
      start,
      most: Some(most),
    } => format!("tail -c +{} -- \"$f\" | head -c {most}", start + 1),
    Span::Last(last) => format!("exec tail -c {last} -- \"$f\""),
  }
}

/// The lines that find the folder of `names` into `$var`, and the entry itself into `$entry_var`.
fn entry(names: &[String], var: &str, entry_var: &str) -> String {
  let (folder, name) = split(names);

  format!(
    "{var}=$(reach {}) || exit $?\n[ -d \"${var}\" ] || exit {}\n{entry_var}=${{{var}%/}}/{}\n",
    words(folder),
    code::NOT_FOLDER,
    quoted(name)
  )
}

impl Shell for Posix {
  fn drives_script(&self) -> String {
    format!(
      "lsblk --json --list --bytes -o NAME,PKNAME,PATH,TYPE,SIZE,MODEL,ROTA,TRAN || exit {denied}
printf '\\0'
findmnt --json --list -o SOURCE,TARGET,FSTYPE,FSROOT || exit {denied}
case $(uname -r) in *[Mm]icrosoft*|*WSL*)
  printf '\\0'
  {WINDOWS_POWERSHELL} -NoProfile -NonInteractive -EncodedCommand {} </dev/null 2>/dev/null ;;
esac
exit 0
",
      encoded_command(WINDOWS_DRIVES),
      denied = code::DENIED,
    )
  }

  fn read_drives(&self, printed: &[u8]) -> Result<Vec<Drive<Rooted>>, String> {
    let printed = String::from_utf8_lossy(printed);
    let parts: Vec<&str> = printed.split('\0').collect();
    let (Some(blocks), Some(mounts)) = (parts.first(), parts.get(1)) else {
      return Err("the drives reading printed nothing readable".into());
    };
    let rooted = |drive: Drive| Drive {
      key: drive.key,
      name: drive.name,
      kind: drive.kind,
      size_bytes: drive.size_bytes,
      volumes: drive
        .volumes
        .into_iter()
        .map(|volume| Rooted {
          root: volume.mount.clone(),
          mount: volume.mount,
          fs_type: volume.fs_type,
        })
        .collect(),
    };
    let linux = read_drives(blocks, mounts)?;

    let Some(windows) = parts.get(2) else {
      return Ok(linux.into_iter().map(rooted).collect());
    };

    // On WSL the Windows disks are the machine's drives, each letter walked where WSL mounts it, and
    // the Linux side gives only the virtual disk it runs from.
    let lettered = windows_mounts(mounts)?;
    let mut drives = read_windows_drives(windows, |letter| {
      lettered
        .iter()
        .find(|(source, _)| source.trim_end_matches(['\\', '/']).eq_ignore_ascii_case(letter))
        .map(|(_, target)| target.clone())
    });

    drives.extend(linux.into_iter().map(rooted).filter_map(|mut drive| {
      drive.volumes.retain(|volume| !WSL_OWN.iter().any(|own| volume.mount.starts_with(own)));

      if drive.volumes.is_empty() {
        return None;
      }

      if drive.kind != Some("network") {
        drive.kind = Some("virtual");
      }

      Some(drive)
    }));

    Ok(drives)
  }

  fn allows(&self, _name: &str) -> bool {
    true
  }

  fn beneath(&self, root: &str, names: &[String]) -> String {
    if names.is_empty() {
      return root.to_owned();
    }

    format!("{}/{}", root.trim_end_matches('/'), names.join("/"))
  }

  fn script(&self, root: &str, op: &Op) -> String {
    let body = match op {
      Op::List(names) => format!(
        "d=$(reach {}) || exit $?
[ -d \"$d\" ] || exit {}
[ -r \"$d\" ] && [ -x \"$d\" ] || exit {}
find -L \"$d\" -mindepth 1 -maxdepth 1 -printf '%y\\t%s\\t%T@\\t%f\\0' 2>/dev/null
exit 0
",
        words(names),
        code::NOT_FOLDER,
        code::DENIED
      ),
      Op::Read(names, span) => format!(
        "f=$(reach {}) || exit $?
[ -d \"$f\" ] && exit {}
[ -r \"$f\" ] || exit {}
size=$(stat -c %s -- \"$f\") || exit {}
printf '%s\\n' \"$size\"
{}
",
        words(names),
        code::IS_FOLDER,
        code::DENIED,
        code::DENIED,
        streamed(*span)
      ),
      Op::MakeFolder(names) => format!(
        "{}guard \"$e\"
there \"$e\" && exit {}
mkdir -- \"$e\" || exit {}
",
        entry(names, "d", "e"),
        code::TAKEN,
        code::DENIED
      ),
      Op::Write(names, source, length) => {
        let (scratch, fill) = match source {
          Source::Stdin => ("".to_owned(), "cat > \"$s\""),
          Source::Scratch(path) => (quoted(path).into_owned(), "cat -- \"$x\" > \"$s\""),
        };

        format!(
          "{}guard \"$e\"
[ -d \"$e\" ] && exit {}
s=${{d%/}}/.bmo-upload-$$
x={scratch}
trap 'rm -f -- \"$s\"; [ -n \"$x\" ] && rm -f -- \"$x\"' EXIT HUP INT TERM PIPE
{fill} || exit {}
[ \"$(stat -c %s -- \"$s\")\" = {length} ] || exit {}
mv -f -- \"$s\" \"$e\" || exit {}
",
          entry(names, "d", "e"),
          code::IS_FOLDER,
          code::DENIED,
          code::CUT_SHORT,
          code::DENIED
        )
      }
      Op::Rename(names, name) => format!(
        "{}there \"$e\" || exit {}
guard \"$e\"
t=${{d%/}}/{}
guard \"$t\"
there \"$t\" && exit {}
mv -- \"$e\" \"$t\" || exit {}
",
        entry(names, "d", "e"),
        code::MISSING,
        quoted(name),
        code::TAKEN,
        code::DENIED
      ),
      Op::Remove(names) => format!(
        "{}there \"$e\" || exit {}
guard \"$e\"
rm -rf -- \"$e\" || exit {}
",
        entry(names, "d", "e"),
        code::MISSING,
        code::DENIED
      ),
      Op::Copy { from, to, moving } => format!(
        "{}there \"$e\" || exit {}
{}{}guard \"$t\"
there \"$t\" && exit {}
[ -d \"$e\" ] && [ ! -L \"$e\" ] && case \"${{t%/}}/\" in \"${{e%/}}/\"*) exit {} ;; esac
{} -- \"$e\" \"$t\" || exit {}
",
        entry(from, "a", "e"),
        code::MISSING,
        if *moving { "guard \"$e\"\n" } else { "" },
        entry(to, "b", "t"),
        code::TAKEN,
        code::INTO_ITSELF,
        if *moving { "mv" } else { "cp -a" },
        code::DENIED
      ),
    };

    format!("{}{body}exit 0\n", self.prelude(root))
  }

  fn login_command(&self, script: &str) -> String {
    format!("sh -c {}", quoted(script))
  }

  fn reads_uploads(&self) -> bool {
    true
  }
}
