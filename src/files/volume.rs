//! A mounted volume walked as files (`docs/files.md`, 「Paths and the fence」): every path is followed to
//! its real location, links and all, and refused where that is not under the volume's own mount. A
//! change lands on an entry itself — its folder followed, its own name kept, so a link is changed as
//! the link it is — and is refused where this machine's guard refuses it.

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use axum::body::Body;

use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

use super::drives::Volume;
use super::{
  Entry, Kind, Opened, Pending, Place, Refused, Span, Upload, Within, cut_short, into_itself, io_refusal,
  is_folder, not_folder, outside, protected, scratch_name, taken,
};

// How long a reading of the disk may take. A share that stopped answering would otherwise hold the
// request for as long as the share does.
const READING: Duration = Duration::from_secs(10);

impl Place for Volume {
  fn id(&self) -> &str {
    &self.place
  }

  fn read_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, Vec<Entry>> {
    Box::pin(read_folder(PathBuf::from(&self.mount), within.clone()))
  }

  fn open<'a>(&'a self, within: &'a Within, span: Span) -> Pending<'a, Opened> {
    Box::pin(open(PathBuf::from(&self.mount), within.clone(), span))
  }

  fn make_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, ()> {
    let (mount, within, volume) = (PathBuf::from(&self.mount), within.clone(), self.clone());

    Box::pin(changing(move || {
      let made = volume.landing(&mount, &within)?;

      fs::create_dir(&made).map_err(|why| change_refusal(&why))
    }))
  }

  fn write<'a>(&'a self, within: &'a Within, upload: Upload) -> Pending<'a, ()> {
    Box::pin(write(self.clone(), PathBuf::from(&self.mount), within.clone(), upload))
  }

  fn rename<'a>(&'a self, within: &'a Within, name: &'a str) -> Pending<'a, ()> {
    let (mount, within, volume, name) =
      (PathBuf::from(&self.mount), within.clone(), self.clone(), name.to_owned());

    Box::pin(changing(move || {
      let from = volume.standing(&mount, &within)?;
      let to = volume.landing(&mount, &within.sibling(&name))?;

      if fs::symlink_metadata(&to).is_ok() {
        return Err(taken());
      }

      fs::rename(&from, &to).map_err(|why| change_refusal(&why))
    }))
  }

  fn remove<'a>(&'a self, within: &'a Within) -> Pending<'a, ()> {
    let (mount, within, volume) = (PathBuf::from(&self.mount), within.clone(), self.clone());

    Box::pin(changing(move || remove_entry(&volume.standing(&mount, &within)?)))
  }

  fn copy_here<'a>(&'a self, from: &'a Within, to: &'a Within, moving: bool) -> Pending<'a, ()> {
    let (mount, from, to, volume) = (PathBuf::from(&self.mount), from.clone(), to.clone(), self.clone());

    Box::pin(changing(move || {
      let source = if moving { volume.standing(&mount, &from)? } else { entry_of(&mount, &from)? };
      let target = volume.landing(&mount, &to)?;

      if target.starts_with(&source) && fs::metadata(&source).is_ok_and(|meta| meta.is_dir()) {
        return Err(into_itself());
      }

      if fs::symlink_metadata(&target).is_ok() {
        return Err(taken());
      }

      if moving {
        match fs::rename(&source, &target) {
          Ok(()) => return Ok(()),
          // Another file system under the same volume's mount: copied, then taken away.
          Err(why) if why.kind() == io::ErrorKind::CrossesDevices => {}
          Err(why) => return Err(change_refusal(&why)),
        }
      }

      copy_tree(&source, &target).map_err(|why| change_refusal(&why))?;

      if moving { remove_entry(&source) } else { Ok(()) }
    }))
  }

  // The folder's real location becomes the mount the fence is judged from, so a link under it leading
  // anywhere else opens nothing.
  fn beneath<'a>(&'a self, top: &'a Within) -> Pending<'a, Box<dyn Place + 'a>> {
    let (mount, top) = (PathBuf::from(&self.mount), top.clone());

    Box::pin(async move {
      let folder = blocking(move || real(&mount, &top)).await?;

      Ok(Box::new(Volume {
        mount: folder.to_string_lossy().into_owned(),
        ..self.clone()
      }) as Box<dyn Place + 'a>)
    })
  }
}

impl Volume {
  /// An entry standing at `within`, refused where the guard refuses changing it.
  fn standing(&self, mount: &Path, within: &Within) -> Result<PathBuf, Refused> {
    let entry = entry_of(mount, within)?;

    if self.guard.refuses(&entry) { Err(protected()) } else { Ok(entry) }
  }

  /// Where a new entry at `within` lands: its folder followed, refused where the guard refuses it.
  fn landing(&self, mount: &Path, within: &Within) -> Result<PathBuf, Refused> {
    let (folder, name) = within.split().ok_or_else(outside)?;
    let folder = real(mount, &folder)?;

    if !folder.is_dir() {
      return Err(not_folder());
    }

    let landing = folder.join(name);

    if self.guard.refuses(&landing) { Err(protected()) } else { Ok(landing) }
  }
}

/// The entry at `within` itself — its folder followed, its own name kept — where it stands.
fn entry_of(mount: &Path, within: &Within) -> Result<PathBuf, Refused> {
  let (folder, name) = within.split().ok_or_else(outside)?;
  let entry = real(mount, &folder)?.join(name);

  fs::symlink_metadata(&entry).map_err(|why| io_refusal(&why))?;

  Ok(entry)
}

/// What a refused change of the file system says, by what went wrong.
fn change_refusal(error: &io::Error) -> Refused {
  match error.kind() {
    io::ErrorKind::AlreadyExists | io::ErrorKind::DirectoryNotEmpty => taken(),
    _ => io_refusal(error),
  }
}

/// A change on the disk, done off the answering threads and run until it ends.
async fn changing(work: impl FnOnce() -> Result<(), Refused> + Send + 'static) -> Result<(), Refused> {
  tokio::task::spawn_blocking(work).await.map_err(Refused::failed)?
}

/// An entry taken away: a folder with everything in it, a link as the link it is.
fn remove_entry(entry: &Path) -> Result<(), Refused> {
  let meta = fs::symlink_metadata(entry).map_err(|why| io_refusal(&why))?;
  let removed = if meta.is_dir() { fs::remove_dir_all(entry) } else { fs::remove_file(entry) };

  removed.map_err(|why| io_refusal(&why))
}

/// A file, a folder with everything in it, or a link as the link it is, copied to `to`.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
  let meta = fs::symlink_metadata(from)?;

  if meta.is_symlink() {
    return symlink(fs::read_link(from)?, to);
  }

  if !meta.is_dir() {
    return fs::copy(from, to).map(|_| ());
  }

  fs::create_dir(to)?;

  for one in fs::read_dir(from)? {
    let one = one?;

    copy_tree(&one.path(), &to.join(one.file_name()))?;
  }

  Ok(())
}

/// A file written as it streams in, beside its final name until all of it has arrived; one standing
/// there already is replaced.
async fn write(volume: Volume, mount: PathBuf, within: Within, upload: Upload) -> Result<(), Refused> {
  let landing = {
    let (volume, mount, within) = (volume.clone(), mount.clone(), within.clone());

    tokio::task::spawn_blocking(move || volume.landing(&mount, &within))
      .await
      .map_err(Refused::failed)??
  };

  if landing.is_dir() {
    return Err(Refused::rule("folder-taken", "a folder of that name stands where the file is to go"));
  }

  let scratch = landing.with_file_name(scratch_name());
  let written = fill(&scratch, upload).await;
  let placed = match written {
    Ok(()) => tokio::fs::rename(&scratch, &landing).await.map_err(|why| io_refusal(&why)),
    Err(refused) => Err(refused),
  };

  if placed.is_err() {
    let _ = tokio::fs::remove_file(&scratch).await;
  }

  placed
}

async fn fill(scratch: &Path, mut upload: Upload) -> Result<(), Refused> {
  let mut file = tokio::fs::File::create_new(scratch).await.map_err(|why| io_refusal(&why))?;
  let mut written: u64 = 0;

  while let Some(chunk) = upload.body.next().await {
    let chunk = chunk.map_err(|_| cut_short())?;

    written += chunk.len() as u64;
    file.write_all(&chunk).await.map_err(|why| io_refusal(&why))?;
  }

  file.flush().await.map_err(|why| io_refusal(&why))?;

  if written == upload.length { Ok(()) } else { Err(cut_short()) }
}

/// Where a path really leads, refused where that is outside the volume.
fn real(mount: &Path, within: &Within) -> Result<PathBuf, Refused> {
  let root = fs::canonicalize(mount).map_err(|why| io_refusal(&why))?;
  let joined = within.names.iter().fold(root.clone(), |path, name| path.join(name));
  let real = fs::canonicalize(&joined).map_err(|why| io_refusal(&why))?;

  if real.starts_with(&root) { Ok(real) } else { Err(outside()) }
}

/// Work on the disk, done off the answering threads and given up past its limit.
async fn blocking<T: Send + 'static>(
  work: impl FnOnce() -> Result<T, Refused> + Send + 'static,
) -> Result<T, Refused> {
  match tokio::time::timeout(READING, tokio::task::spawn_blocking(work)).await {
    Ok(Ok(done)) => done,
    Ok(Err(why)) => Err(Refused::failed(why)),
    Err(_) => Err(Refused::failed("the disk did not answer in time")),
  }
}

async fn read_folder(mount: PathBuf, within: Within) -> Result<Vec<Entry>, Refused> {
  blocking(move || {
    let folder = real(&mount, &within)?;

    if !folder.is_dir() {
      return Err(not_folder());
    }

    let read = fs::read_dir(&folder).map_err(|why| io_refusal(&why))?;

    Ok(
      read
        .flatten()
        .map(|one| entry(&one.path(), one.file_name().to_string_lossy().into_owned()))
        .collect(),
    )
  })
  .await
}

async fn open(mount: PathBuf, within: Within, span: Span) -> Result<Opened, Refused> {
  let name = within.last().unwrap_or_default().to_owned();
  let (real, size) = blocking(move || {
    let real = real(&mount, &within)?;
    let meta = fs::metadata(&real).map_err(|why| io_refusal(&why))?;

    if meta.is_dir() {
      return Err(is_folder());
    }

    Ok((real, meta.len()))
  })
  .await?;
  let mut file = tokio::fs::File::open(&real).await.map_err(|why| io_refusal(&why))?;
  let (start, length) = span.within(size);

  if start > 0 {
    file.seek(io::SeekFrom::Start(start)).await.map_err(|why| io_refusal(&why))?;
  }

  Ok(Opened {
    name,
    size,
    start,
    length,
    body: Body::from_stream(ReaderStream::new(file.take(length))),
  })
}

/// One entry as the folder holds it, its link followed where it is one.
fn entry(path: &Path, name: String) -> Entry {
  let Ok(meta) = fs::metadata(path) else {
    return Entry {
      name,
      kind: Kind::Other,
      size: None,
      modified: None,
    };
  };
  let modified = meta
    .modified()
    .ok()
    .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
    .and_then(|since| i64::try_from(since.as_millis()).ok());

  if meta.is_dir() {
    Entry {
      name,
      kind: Kind::Folder,
      size: None,
      modified,
    }
  } else if meta.is_file() {
    Entry {
      name,
      kind: Kind::File,
      size: Some(meta.len()),
      modified,
    }
  } else {
    Entry {
      name,
      kind: Kind::Other,
      size: None,
      modified,
    }
  }
}
