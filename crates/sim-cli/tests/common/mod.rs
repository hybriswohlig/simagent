//! A lock of a run held or tried by another process, through perl's `flock` (the call of
//! `File::try_lock` on Linux and macOS). A test cannot hold a lock in its own process and
//! know when it is free again: on Linux every child a parallel test spawns gets a copy of
//! the open file until its `exec`, and `flock` lets go only when the last copy is closed.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// A process that holds the lock at `lock` until it is dropped.
pub struct Holder(Child);

impl Holder {
    pub fn hold(lock: &Path) -> Holder {
        let mut child = Command::new("perl")
            .args([
                "-e",
                "use Fcntl qw(:flock); open(my $f, '>>', $ARGV[0]) or die; flock($f, LOCK_EX) or die; $| = 1; print qq(held\\n); sleep 60",
            ])
            .arg(lock)
            .stdout(Stdio::piped())
            .spawn()
            .expect("perl runs");
        let mut line = String::new();
        BufReader::new(child.stdout.take().expect("its output"))
            .read_line(&mut line)
            .expect("reads");
        assert_eq!(line, "held\n");
        Holder(child)
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Whether another process gets the lock at `lock` at once.
pub fn free(lock: &Path) -> bool {
    Command::new("perl")
        .args([
            "-e",
            "use Fcntl qw(:flock); open(my $f, '>>', $ARGV[0]) or die; exit(flock($f, LOCK_EX | LOCK_NB) ? 0 : 1)",
        ])
        .arg(lock)
        .status()
        .expect("perl runs")
        .success()
}
