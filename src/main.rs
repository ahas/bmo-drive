//! `bmo-drive`: the service and its command in one binary (`docs/api.md`, 「The command」).

use std::process::ExitCode;

use bmo_drive::command::{self, Failure, NOUNS, ShareArgs};
use bmo_drive::settings::Settings;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bmo-drive", version, about = "The file viewer and its share links")]
struct Command {
  #[command(subcommand)]
  word: Word,
}

#[derive(Subcommand)]
enum Word {
  /// Opens the store and answers the routes.
  Serve,
  /// Whether the service the settings name answers ready; ends 0 where it does.
  Health,
  /// The nouns this command carries, one a line, as `bmo` asks a unit command.
  Nouns,
  /// The file viewer's entries.
  File {
    #[command(subcommand)]
    word: FileWord,
  },
  /// A share link for a file or folder, as `file share` makes it.
  #[command(name = "file-share")]
  FileShare(ShareArgs),
}

#[derive(Subcommand)]
enum FileWord {
  /// A share link for a file or folder of the file viewer, a path on this machine or one inside a place;
  /// the links standing with --list, and one taken away with --remove.
  Share(ShareArgs),
}

#[tokio::main]
async fn main() -> ExitCode {
  let command = Command::parse();

  if let Word::Nouns = command.word {
    NOUNS.iter().for_each(|noun| println!("{noun}"));
    return ExitCode::SUCCESS;
  }

  let settings = match Settings::load() {
    Ok(settings) => settings,
    Err(why) => {
      eprintln!("bmo-drive: {why}");
      return ExitCode::FAILURE;
    }
  };
  let done = match command.word {
    Word::Serve => bmo_drive::serve(settings).await,
    Word::Health => bmo_drive::health::ask(&settings).await,
    Word::Nouns => Ok(()),
    Word::File {
      word: FileWord::Share(asked),
    }
    | Word::FileShare(asked) => return shared(command::file_share(settings, asked).await),
  };

  match done {
    Ok(()) => ExitCode::SUCCESS,
    Err(why) => {
      eprintln!("bmo-drive: {why}");
      ExitCode::FAILURE
    }
  }
}

/// What `file share` printed, and how it ended.
fn shared(done: Result<Vec<String>, Failure>) -> ExitCode {
  match done {
    Ok(lines) => {
      lines.iter().for_each(|line| println!("{line}"));
      ExitCode::SUCCESS
    }
    Err(Failure::Partly(lines)) => {
      lines.iter().for_each(|line| println!("{line}"));
      ExitCode::FAILURE
    }
    Err(Failure::Refused(why)) => {
      eprintln!("{why}");
      ExitCode::FAILURE
    }
  }
}
