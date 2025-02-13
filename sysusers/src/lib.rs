//! Parse and generate systemd sysusers.d entries.
// SPDX-License-Identifier: Apache-2.0 OR MIT

#[allow(dead_code)]
mod nameservice;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use camino::Utf8Path;
use cap_std_ext::dirext::{CapStdExtDirExt, CapStdExtDirExtUtf8};
use cap_std_ext::{cap_std::fs::Dir, cap_std::fs_utf8::Dir as DirUtf8};
use thiserror::Error;

const SYSUSERSD: &str = "usr/lib/sysusers.d";

/// An error when processing sysusers
#[derive(Debug, Error)]
#[allow(missing_docs)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("I/O error on {path}: {err}")]
    PathIo { path: PathBuf, err: std::io::Error },
    #[error("Failed to parse sysusers entry: {0}")]
    ParseFailure(String),
    #[error("Failed to parse sysusers entry from {path}: {err}")]
    ParseFailureInFile { path: PathBuf, err: String },
    #[error("Failed to load etc/passwd: {0}")]
    PasswdLoadFailure(String),
    #[error("Failed to load etc/group: {0}")]
    GroupLoadFailure(String),
}

/// The type of Result.
pub type Result<T> = std::result::Result<T, Error>;

/// A parsed sysusers.d entry
#[derive(Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SysusersEntry {
    /// Defines a user
    User {
        name: String,
        uid: Option<u32>,
        pgid: Option<u32>,
        gecos: String,
        home: Option<String>,
        shell: Option<String>,
    },
    /// Defines a group
    Group { name: String, id: Option<u32> },
    /// Defines a range of uids
    Range { start: u32, end: u32 },
}

impl SysusersEntry {
    /// Given an input string, finds the next "token" which is normally delimited by
    /// whitespace, but "quoted strings" are also supported. Returns that token
    /// and the remainder. If there are no more tokens, this returns None.
    ///
    /// Yes this is a lot of manual parsing and there's a ton of crates we could use,
    /// like winnow, but this problem domain is *just* simple enough that I decided
    /// not to learn that yet.
    fn next_token(s: &str) -> Option<(&str, &str)> {
        let s = s.trim_start();
        let (first, rest) = match s.strip_prefix('"') {
            None => {
                let idx = s
                    .find(|c: char| c.is_whitespace())
                    .unwrap_or(s.as_bytes().len());
                s.split_at(idx)
            }
            Some(rest) => {
                let Some(end) = rest.find(|c: char| c == '"') else {
                    return None;
                };
                (&rest[..end], &rest[end + 1..])
            }
        };
        if first.is_empty() {
            None
        } else {
            Some((first, rest))
        }
    }

    fn next_token_owned(s: &str) -> Option<(String, &str)> {
        Self::next_token(s).map(|(a, b)| (a.to_owned(), b))
    }

    fn next_optional_token(s: &str) -> Option<(Option<&str>, &str)> {
        let (token, s) = Self::next_token(s)?;
        let token = Some(token).filter(|t| *t != "-");
        Some((token, s))
    }

    fn next_optional_token_owned(s: &str) -> Option<(Option<String>, &str)> {
        Self::next_optional_token(s).map(|(a, b)| (a.map(|v| v.to_owned()), b))
    }

    pub(crate) fn parse(s: &str) -> Result<Option<SysusersEntry>> {
        let err = || Error::ParseFailure(s.to_owned());
        let (ftype, s) = Self::next_token(s).ok_or_else(err.clone())?;
        let r = match ftype {
            "u" => {
                let (name, s) = Self::next_token_owned(s).ok_or_else(err.clone())?;
                let (id, s) = Self::next_optional_token(s).ok_or_else(err.clone())?;
                let (uid, pgid) = id
                    .and_then(|v| v.split_once(':'))
                    .or_else(|| id.map(|id| (id, id)))
                    .map(|(uid, gid)| (Some(uid), Some(gid)))
                    .unwrap_or((None, None));
                let uid = uid.map(|id| id.parse()).transpose().map_err(|_| err())?;
                let pgid = pgid.map(|id| id.parse()).transpose().map_err(|_| err())?;
                let (gecos, s) = Self::next_token_owned(s).ok_or_else(err.clone())?;
                let (home, s) = Self::next_optional_token_owned(s).unwrap_or_default();
                let (shell, _) = Self::next_optional_token_owned(s).unwrap_or_default();
                SysusersEntry::User {
                    name,
                    uid,
                    pgid,
                    gecos,
                    home,
                    shell,
                }
            }
            "g" => {
                let (name, s) = Self::next_token_owned(s).ok_or_else(err.clone())?;
                let (id, _) = Self::next_optional_token(s).ok_or_else(err.clone())?;
                let id = id.map(|id| id.parse()).transpose().map_err(|_| err())?;
                SysusersEntry::Group { name, id }
            }
            "r" => {
                let (_, s) = Self::next_optional_token(s).ok_or_else(err.clone())?;
                let (range, _) = Self::next_token(s).ok_or_else(err.clone())?;
                let (start, end) = range.split_once('-').ok_or_else(err.clone())?;
                let start: u32 = start.parse().map_err(|_| err())?;
                let end: u32 = end.parse().map_err(|_| err())?;
                SysusersEntry::Range { start, end }
            }
            // In the case of a sysusers entry that is of unknown type, we skip it out of conservatism
            _ => return Ok(None),
        };
        Ok(Some(r))
    }
}

/// Read all tmpfiles.d entries in the target directory, and return a mapping
/// from (file path) => (single tmpfiles.d entry line)
pub fn read_sysusers(rootfs: &Dir) -> Result<Vec<SysusersEntry>> {
    let Some(d) = rootfs.open_dir_optional(SYSUSERSD)? else {
        return Ok(Default::default());
    };
    let d = DirUtf8::from_cap_std(d);
    let mut result = Vec::new();
    let mut found_users = BTreeSet::new();
    let mut found_groups = BTreeSet::new();
    for name in d.filenames_sorted()? {
        let Some("conf") = Utf8Path::new(&name).extension() else {
            continue;
        };
        let r = d.open(&name).map(BufReader::new)?;
        for line in r.lines() {
            let line = line?;
            if line.is_empty() || line.starts_with("#") {
                continue;
            }
            let Some(e) = SysusersEntry::parse(&line).map_err(|e| Error::ParseFailureInFile {
                path: name.clone().into(),
                err: e.to_string(),
            })?
            else {
                continue;
            };
            match e {
                SysusersEntry::User {
                    ref name, ref pgid, ..
                } if !found_users.contains(name.as_str()) => {
                    found_users.insert(name.clone());
                    found_groups.insert(name.clone());
                    // Users implicitly create a group with the same name
                    result.push(SysusersEntry::Group {
                        name: name.clone(),
                        id: pgid.clone(),
                    });
                    result.push(e);
                }
                SysusersEntry::Group { ref name, .. } if !found_groups.contains(name.as_str()) => {
                    found_groups.insert(name.clone());
                    result.push(e);
                }
                _ => {
                    // Ignore others.
                }
            }
        }
    }
    Ok(result)
}

/// The result of analyzing /etc/{passwd,group} in a root vs systemd-sysusers.
#[derive(Debug)]
pub struct SysusersAnalysis {
    /// Entries which are found in /etc/passwd but not present in systemd-sysusers.
    pub missing_users: BTreeSet<String>,
    /// Entries which are found in /etc/group but not present in systemd-sysusers.
    pub missing_groups: BTreeSet<String>,
}

impl SysusersAnalysis {
    /// Returns true if this analysis finds no missing entries.
    pub fn is_empty(&self) -> bool {
        self.missing_users.is_empty() && self.missing_groups.is_empty()
    }
}

/// Analyze the state of /etc/passwd vs systemd-sysusers.
pub fn analyze(rootfs: &Dir) -> Result<SysusersAnalysis> {
    struct SysuserData {
        #[allow(dead_code)]
        uid: Option<u32>,
        #[allow(dead_code)]
        pgid: Option<u32>,
    }

    struct SysgroupData {
        #[allow(dead_code)]
        id: Option<u32>,
    }

    let mut passwd = nameservice::passwd::load_etc_passwd(rootfs)
        .map_err(|e| Error::PasswdLoadFailure(e.to_string()))?
        .into_iter()
        .map(|mut e| {
            // Make the name be the map key, leaving the old value a stub
            let mut name = String::new();
            std::mem::swap(&mut e.name, &mut name);
            (name, e)
        })
        .collect::<BTreeMap<_, _>>();
    let mut group = nameservice::group::load_etc_group(rootfs)
        .map_err(|e| Error::GroupLoadFailure(e.to_string()))?
        .into_iter()
        .map(|mut e| {
            // Make the name be the map key, leaving the old value a stub
            let mut name = String::new();
            std::mem::swap(&mut e.name, &mut name);
            (name, e)
        })
        .collect::<BTreeMap<_, _>>();

    let (sysusers_users, sysusers_groups) = {
        let mut users = BTreeMap::new();
        let mut groups = BTreeMap::new();
        for ent in read_sysusers(rootfs)? {
            match ent {
                SysusersEntry::User {
                    name, uid, pgid, ..
                } => {
                    users.insert(name, SysuserData { uid, pgid });
                }
                SysusersEntry::Group { name, id } => {
                    groups.insert(name, SysgroupData { id });
                }
                SysusersEntry::Range { .. } => {
                    // Nothing to do here
                }
            }
        }
        (users, groups)
    };

    passwd.retain(|k, _| !sysusers_users.contains_key(k.as_str()));
    group.retain(|k, _| !sysusers_groups.contains_key(k.as_str()));

    Ok(SysusersAnalysis {
        missing_users: passwd.into_keys().collect(),
        missing_groups: group.into_keys().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;

    use anyhow::Result;
    use cap_std_ext::cap_std;
    use indoc::indoc;

    const SYSUSERS_REF: &str = indoc::indoc! { r##"
        # Comment here
        u root 0 "Super User" /root /bin/bash
        # This one omits the shell
        u root    0     "Super User" /root
        u bin 1:1 "bin" /bin -
        # Another comment
        u daemon 2:2 "daemon" /sbin -
        u adm 3:4 "adm" /var/adm -
        u lp 4:7 "lp" /var/spool/lpd -
        u sync 5:0 "sync" /sbin /bin/sync
        u shutdown 6:0 "shutdown" /sbin /sbin/shutdown
        u halt 7:0 "halt" /sbin /sbin/halt
        u mail 8:12 "mail" /var/spool/mail -
        u operator 11:0 "operator" /root -
        u games 12:100 "games" /usr/games -
        u ftp 14:50 "FTP User" /var/ftp -
        u nobody 65534:65534 "Kernel Overflow User" - -
    "##};

    const SYSGROUPS_REF: &str = indoc::indoc! { r##"
        # A comment here
        g root 0
        g bin 1
        g daemon 2
        g sys 3
        g adm 4
        g tty 5
        g disk 6
        g lp 7
        g mem 8
        g kmem 9
        g wheel 10
        g cdrom 11
        g mail 12
        g man 15
        g dialout 18
        g floppy 19
        g games 20
        g utmp 22
        g tape 33
        g kvm 36
        g video 39
        g ftp 50
        g lock 54
        g audio 63
        g users 100
        g clock 103
        g input 104
        g render 105
        g sgx 106
        g nobody 65534
    "##};

    #[test]
    fn test_sysusers_parse() -> Result<()> {
        let mut entries = SYSUSERS_REF
            .lines()
            .filter(|line| !(line.is_empty() || line.starts_with('#')))
            .map(|line| SysusersEntry::parse(line).unwrap().unwrap());
        assert_eq!(
            entries.next().unwrap(),
            SysusersEntry::User {
                name: "root".into(),
                uid: Some(0),
                pgid: Some(0),
                gecos: "Super User".into(),
                home: Some("/root".into()),
                shell: Some("/bin/bash".into())
            }
        );
        assert_eq!(
            entries.next().unwrap(),
            SysusersEntry::User {
                name: "root".into(),
                uid: Some(0),
                pgid: Some(0),
                gecos: "Super User".into(),
                home: Some("/root".into()),
                shell: None
            }
        );
        assert_eq!(
            entries.next().unwrap(),
            SysusersEntry::User {
                name: "bin".into(),
                uid: Some(1),
                pgid: Some(1),
                gecos: "bin".into(),
                home: Some("/bin".into()),
                shell: None
            }
        );
        let _ = entries.next().unwrap();
        assert_eq!(
            entries.next().unwrap(),
            SysusersEntry::User {
                name: "adm".into(),
                uid: Some(3),
                pgid: Some(4),
                gecos: "adm".into(),
                home: Some("/var/adm".into()),
                shell: None
            }
        );
        assert_eq!(entries.count(), 9);
        Ok(())
    }

    #[test]
    fn test_sysgroups_parse() -> Result<()> {
        let mut entries = SYSGROUPS_REF
            .lines()
            .filter(|line| !(line.is_empty() || line.starts_with('#')))
            .map(|line| SysusersEntry::parse(line).unwrap().unwrap());
        assert_eq!(
            entries.next().unwrap(),
            SysusersEntry::Group {
                name: "root".into(),
                id: Some(0),
            }
        );
        assert_eq!(
            entries.next().unwrap(),
            SysusersEntry::Group {
                name: "bin".into(),
                id: Some(1),
            }
        );
        assert_eq!(entries.count(), 28);
        Ok(())
    }

    fn newroot() -> Result<cap_std_ext::cap_tempfile::TempDir> {
        let root = cap_std_ext::cap_tempfile::tempdir(cap_std::ambient_authority())?;
        root.create_dir("etc")?;
        root.write("etc/passwd", b"")?;
        root.write("etc/group", b"")?;
        root.create_dir_all(SYSUSERSD)?;
        root.atomic_replace_with(
            Utf8Path::new(SYSUSERSD).join("setup.conf"),
            |w| -> std::io::Result<()> {
                w.write_all(SYSUSERS_REF.as_bytes())?;
                w.write_all(SYSGROUPS_REF.as_bytes())?;
                Ok(())
            },
        )?;
        Ok(root)
    }

    #[test]
    fn test_missing() -> Result<()> {
        let root = &newroot()?;

        let a = analyze(&root).unwrap();
        assert!(a.is_empty());

        root.write(
            "etc/passwd",
            indoc! { r#"
            root:x:0:0:Super User:/root:/bin/bash
            passim:x:982:982:Local Caching Server:/usr/share/empty:/usr/bin/nologin
            avahi:x:70:70:Avahi mDNS/DNS-SD Stack:/var/run/avahi-daemon:/sbin/nologin
        "#},
        )?;
        root.write(
            "etc/group",
            indoc! { r#"
            root:x:0:
            adm:x:4:
            wheel:x:10:
            sudo:x:16:
            systemd-journal:x:190:
            printadmin:x:983:
            rpc:x:32:
            passim:x:982:
            avahi:x:70:
            sshd:x:981:
        "#},
        )?;

        let a = analyze(&root).unwrap();
        assert!(!a.is_empty());
        let missing = a.missing_users.iter().map(|s| s.as_str());
        assert!(missing.eq(["avahi", "passim"]));
        let missing = a.missing_groups.iter().map(|s| s.as_str());
        assert!(missing.eq([
            "avahi",
            "passim",
            "printadmin",
            "rpc",
            "sshd",
            "sudo",
            "systemd-journal"
        ]));

        Ok(())
    }
}
