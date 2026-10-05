//! macFUSE-free mount backend: a localhost NFSv3 server that forwards every
//! operation to the remote host over the system `ssh -s <host> sftp`
//! subsystem. macOS mounts it with its built-in NFS client, so neither the
//! Mac nor the remote host needs extra software or privileges.
//!
//! The server listens on 127.0.0.1 only and exits once its volume leaves the
//! mount table. SSH reuses the user's keys, agent and `~/.ssh/config`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use nfsserve::{
    nfs::*,
    tcp::{NFSTcp, NFSTcpListener},
    vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities},
};
use russh_sftp::{
    client::{RawSftpSession, error::Error as SftpError},
    protocol::{FileAttributes, OpenFlags, Packet, StatusCode},
};
use tokio::sync::Mutex as AsyncMutex;
use unicode_normalization::UnicodeNormalization;

/// One SFTP request; OpenSSH accepts larger, but 32 KiB is universally safe.
const CHUNK: usize = 32 * 1024;
/// NFS transfer size advertised to macOS: each READ/WRITE fans out into
/// `TRANSFER / CHUNK` pipelined SFTP requests, which hides network latency.
const TRANSFER: u32 = 1 << 20;
/// Answer before the kernel's soft-mount timeout so the bridge, not the
/// kernel, reports a dead link (see the `timeo` mount option).
const OPERATION_TIMEOUT: Duration = Duration::from_secs(15);
/// Metadata and directory listings are reused this long, like `actimeo`.
const CACHE_TTL: Duration = Duration::from_secs(1);
/// Read handles stay open this long after their last use. A remote
/// replacement becomes visible once the idle handle is closed.
const HANDLE_TTL: Duration = Duration::from_secs(2);
const MAX_HANDLES: usize = 64;
/// Cache sizes beyond which expired entries are pruned.
const MAX_CACHED_ATTRS: usize = 4096;
const MAX_CACHED_LISTINGS: usize = 256;

/// Mount options for macOS `mount_nfs` against this server on `port`.
pub fn mount_options(port: u16) -> String {
    format!(
        "nolocks,vers=3,tcp,soft,intr,timeo=100,retrans=10,actimeo=1,readahead=16,\
         rsize={TRANSFER},wsize={TRANSFER},port={port},mountport={port}"
    )
}

/// Serve `host:remote_root` at `mount_root` until the volume is unmounted.
pub fn serve(host: &str, remote_root: &str, mount_root: &Path, export: &str) -> Result<(), String> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| format!("start runtime: {e}"))?
        .block_on(run(host, remote_root, mount_root, export))
}

async fn run(host: &str, remote_root: &str, mount_root: &Path, export: &str) -> Result<(), String> {
    let link = Link::new(host);
    let root = remote_root.trim_end_matches('/');
    let root = if root.is_empty() { "/" } else { root }.to_owned();
    let attrs = link
        .call(|s| {
            let root = root.clone();
            async move { s.stat(root).await }
        })
        .await
        .map_err(|e| format!("remote root {root}: {e:?}"))?;
    if !matches!(kind(&attrs.attrs), ftype3::NF3DIR) {
        return Err(format!("remote root {root} is not a directory"));
    }
    let fs = SftpFs::new(link, root, unsafe { libc::getuid() }, unsafe {
        libc::getgid()
    });
    let mut listener = NFSTcpListener::bind("127.0.0.1:0", fs)
        .await
        .map_err(|e| format!("listen on localhost: {e}"))?;
    listener.with_export_name(export);
    let port = listener.get_listen_port();
    tokio::spawn(async move { listener.handle_forever().await });

    let status = tokio::process::Command::new("/sbin/mount_nfs")
        .args(["-o", &mount_options(port)])
        .arg(format!("127.0.0.1:/{export}"))
        .arg(mount_root)
        .status()
        .await
        .map_err(|e| format!("start mount_nfs: {e}"))?;
    if !status.success() {
        return Err(format!("mount_nfs failed: {status}"));
    }
    eprintln!(
        "NFS bridge for {host}:{remote_root} mounted at {}",
        mount_root.display()
    );
    // The OS unmount (Finder eject, `rws disconnect`, logout) ends the bridge.
    let source = format!("127.0.0.1:/{export}");
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        match crate::lifecycle::identity(mount_root) {
            Ok(Some(id)) if id.source == source => {}
            Ok(_) => {
                eprintln!("volume unmounted; NFS bridge exiting");
                return Ok(());
            }
            Err(e) => eprintln!("inspect mount table: {e}"),
        }
    }
}

/// The SFTP connection, re-established on demand after a transport failure
/// (sleep, network change). File ids are path-based, so they survive it.
struct Link {
    host: String,
    session: AsyncMutex<Option<Arc<RawSftpSession>>>,
    /// Incremented on every new session: SFTP handles die with their session.
    epoch: AtomicUsize,
}

#[derive(Debug)]
enum CallError {
    Status(StatusCode),
    Transport(String),
}

impl Link {
    async fn connect(&self) -> Result<Arc<RawSftpSession>, CallError> {
        let mut child = tokio::process::Command::new("ssh")
            .args([
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=10",
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                "-s",
                "--",
                &self.host,
                "sftp",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| CallError::Transport(format!("start ssh: {e}")))?;
        let stream = tokio::io::join(child.stdout.take().unwrap(), child.stdin.take().unwrap());
        let session = RawSftpSession::new(stream);
        session.set_timeout(OPERATION_TIMEOUT.as_secs());
        tokio::time::timeout(OPERATION_TIMEOUT, session.init())
            .await
            .map_err(|_| CallError::Transport("SFTP handshake timed out".into()))?
            .map_err(|e| CallError::Transport(format!("SFTP handshake: {e}")))?;
        // The child is reaped by the runtime; killing it closes the session.
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        Ok(Arc::new(session))
    }

    async fn session(&self) -> Result<Arc<RawSftpSession>, CallError> {
        let mut current = self.session.lock().await;
        if let Some(session) = current.as_ref() {
            return Ok(session.clone());
        }
        let session = self.connect().await?;
        self.epoch.fetch_add(1, Ordering::SeqCst);
        *current = Some(session.clone());
        Ok(session)
    }

    /// Run one SFTP request. A transport failure drops the session so the
    /// next request reconnects; the failed request is never retried, since
    /// it may already have taken effect remotely.
    async fn call<T, F, Fut>(&self, op: F) -> Result<T, CallError>
    where
        F: FnOnce(Arc<RawSftpSession>) -> Fut,
        Fut: std::future::Future<Output = Result<T, SftpError>>,
    {
        let session = self.session().await?;
        let result = tokio::time::timeout(OPERATION_TIMEOUT, op(session.clone())).await;
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(SftpError::Status(status))) => Err(CallError::Status(status.status_code)),
            Ok(Err(e)) => {
                self.drop_session(&session).await;
                Err(CallError::Transport(e.to_string()))
            }
            Err(_) => {
                self.drop_session(&session).await;
                Err(CallError::Transport("SFTP request timed out".into()))
            }
        }
    }

    async fn drop_session(&self, failed: &Arc<RawSftpSession>) {
        let mut current = self.session.lock().await;
        if current.as_ref().is_some_and(|s| Arc::ptr_eq(s, failed)) {
            *current = None;
        }
    }

    fn generation(&self) -> usize {
        self.epoch.load(Ordering::SeqCst)
    }

    fn new(host: &str) -> Self {
        Self {
            host: host.to_owned(),
            session: AsyncMutex::new(None),
            epoch: AtomicUsize::new(0),
        }
    }
}

fn nfs_error(e: CallError) -> nfsstat3 {
    match e {
        CallError::Status(StatusCode::NoSuchFile) => nfsstat3::NFS3ERR_NOENT,
        CallError::Status(StatusCode::PermissionDenied) => nfsstat3::NFS3ERR_ACCES,
        CallError::Status(StatusCode::OpUnsupported) => nfsstat3::NFS3ERR_NOTSUPP,
        CallError::Status(StatusCode::Eof) => nfsstat3::NFS3ERR_IO,
        CallError::Status(_) => nfsstat3::NFS3ERR_IO,
        CallError::Transport(reason) => {
            eprintln!("SFTP transport error: {reason}");
            nfsstat3::NFS3ERR_IO
        }
    }
}

/// Bidirectional map between NFS file ids and paths relative to the root.
struct Ids {
    paths: HashMap<fileid3, String>,
    by_path: HashMap<String, fileid3>,
    next: fileid3,
}

impl Ids {
    fn new() -> Self {
        let mut ids = Self {
            paths: HashMap::new(),
            by_path: HashMap::new(),
            next: 2,
        };
        ids.paths.insert(1, String::new());
        ids.by_path.insert(String::new(), 1);
        ids
    }

    fn id(&mut self, rel: &str) -> fileid3 {
        if let Some(id) = self.by_path.get(rel) {
            return *id;
        }
        let id = self.next;
        self.next += 1;
        self.paths.insert(id, rel.to_owned());
        self.by_path.insert(rel.to_owned(), id);
        id
    }

    fn forget(&mut self, rel: &str) -> Option<fileid3> {
        let id = self.by_path.remove(rel)?;
        self.paths.remove(&id);
        Some(id)
    }

    /// Move `from` and everything below it to `to`, keeping ids stable.
    fn rename(&mut self, from: &str, to: &str) {
        self.forget(to);
        let prefix = format!("{from}/");
        let moved: Vec<(fileid3, String)> = self
            .paths
            .iter()
            .filter(|(_, p)| p.as_str() == from || p.starts_with(&prefix))
            .map(|(id, p)| (*id, format!("{to}{}", &p[from.len()..])))
            .collect();
        for (id, new) in moved {
            if let Some(old) = self.paths.insert(id, new.clone()) {
                self.by_path.remove(&old);
            }
            self.by_path.insert(new, id);
        }
    }
}

/// AppleDouble (`._name`) and `.DS_Store` files carry macOS-only metadata;
/// they stay in memory on the Mac and never reach the remote host.
fn is_local(rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    base.starts_with("._") || base == ".DS_Store"
}

/// Validate a name from the client and compose it (NFC): macOS may send
/// decomposed names, while RWS stores remote names composed.
fn component(name: &filename3) -> Result<String, nfsstat3> {
    let s = std::str::from_utf8(&name.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;
    if s.is_empty() || s == "." || s == ".." || s.contains('/') || s.contains('\0') {
        return Err(nfsstat3::NFS3ERR_INVAL);
    }
    Ok(s.nfc().collect())
}

fn child(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

fn parent(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

fn kind(a: &FileAttributes) -> ftype3 {
    match a.permissions.unwrap_or(0) & 0o170000 {
        0o040000 => ftype3::NF3DIR,
        0o120000 => ftype3::NF3LNK,
        _ => ftype3::NF3REG,
    }
}

fn ssh_string(s: &str, out: &mut Vec<u8>) {
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

struct Handle {
    handle: String,
    generation: usize,
    used: Instant,
}

struct Listing {
    at: Instant,
    entries: Vec<(String, FileAttributes)>,
}

struct SftpFs {
    link: Link,
    root: String,
    uid: u32,
    gid: u32,
    ids: Mutex<Ids>,
    attrs: Mutex<HashMap<fileid3, (Instant, FileAttributes)>>,
    listings: Mutex<HashMap<fileid3, Listing>>,
    handles: Mutex<HashMap<fileid3, Handle>>,
    local: Mutex<HashMap<String, Vec<u8>>>,
}

impl SftpFs {
    fn new(link: Link, root: String, uid: u32, gid: u32) -> Self {
        Self {
            link,
            root,
            uid,
            gid,
            ids: Mutex::new(Ids::new()),
            attrs: Mutex::new(HashMap::new()),
            listings: Mutex::new(HashMap::new()),
            handles: Mutex::new(HashMap::new()),
            local: Mutex::new(HashMap::new()),
        }
    }

    fn rel(&self, id: fileid3) -> Result<String, nfsstat3> {
        self.ids
            .lock()
            .unwrap()
            .paths
            .get(&id)
            .cloned()
            .ok_or(nfsstat3::NFS3ERR_STALE)
    }

    fn id(&self, rel: &str) -> fileid3 {
        self.ids.lock().unwrap().id(rel)
    }

    fn abs(&self, rel: &str) -> String {
        match (self.root.as_str(), rel) {
            (root, "") => root.to_owned(),
            ("/", rel) => format!("/{rel}"),
            (root, rel) => format!("{root}/{rel}"),
        }
    }

    fn fattr(&self, id: fileid3, a: &FileAttributes) -> fattr3 {
        let size = a.size.unwrap_or(0);
        let time = |s: Option<u32>| nfstime3 {
            seconds: s.unwrap_or(0),
            nseconds: 0,
        };
        fattr3 {
            ftype: kind(a),
            mode: a.permissions.unwrap_or(0) & 0o7777,
            nlink: 1,
            // Present remote files as the local user's: the remote server
            // enforces the SSH account's real permissions on every request.
            uid: self.uid,
            gid: self.gid,
            size,
            used: size,
            rdev: specdata3::default(),
            fsid: 0,
            fileid: id,
            atime: time(a.atime),
            mtime: time(a.mtime),
            ctime: time(a.mtime),
        }
    }

    /// Drop cached state for `rel` and its parent listing after a change.
    fn changed(&self, rel: &str) {
        let (id, parent_id) = {
            let ids = self.ids.lock().unwrap();
            (
                ids.by_path.get(rel).copied(),
                ids.by_path.get(parent(rel)).copied(),
            )
        };
        let mut attrs = self.attrs.lock().unwrap();
        let mut listings = self.listings.lock().unwrap();
        for id in [id, parent_id].into_iter().flatten() {
            attrs.remove(&id);
            listings.remove(&id);
        }
    }

    async fn lstat(&self, id: fileid3, rel: &str) -> Result<FileAttributes, nfsstat3> {
        if let Some((at, a)) = self.attrs.lock().unwrap().get(&id)
            && at.elapsed() < CACHE_TTL
        {
            return Ok(a.clone());
        }
        let mut attempt = 0;
        let a = loop {
            let path = self.abs(rel);
            match self.link.call(|s| async move { s.lstat(path).await }).await {
                Ok(a) => break a.attrs,
                Err(CallError::Transport(_)) if attempt == 0 => attempt += 1,
                Err(e) => return Err(nfs_error(e)),
            }
        };
        let mut attrs = self.attrs.lock().unwrap();
        if attrs.len() >= MAX_CACHED_ATTRS {
            attrs.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
        }
        attrs.insert(id, (Instant::now(), a.clone()));
        Ok(a)
    }

    fn local_attr(&self, id: fileid3, rel: &str) -> Result<fattr3, nfsstat3> {
        let len = self
            .local
            .lock()
            .unwrap()
            .get(rel)
            .map(Vec::len)
            .ok_or(nfsstat3::NFS3ERR_NOENT)?;
        let mut a = FileAttributes::empty();
        a.permissions = Some(0o100644);
        a.size = Some(len as u64);
        Ok(self.fattr(id, &a))
    }

    async fn stat(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        let rel = self.rel(id)?;
        if is_local(&rel) {
            return self.local_attr(id, &rel);
        }
        let a = self.lstat(id, &rel).await?;
        Ok(self.fattr(id, &a))
    }

    async fn status(
        &self,
        op: impl AsyncFnOnce(Arc<RawSftpSession>) -> Result<(), SftpError>,
    ) -> Result<(), nfsstat3> {
        self.link
            .call(|s| async move { op(s).await })
            .await
            .map_err(nfs_error)
    }

    async fn apply(&self, path: &str, set: &sattr3) -> Result<(), nfsstat3> {
        let mut a = FileAttributes::empty();
        if let set_mode3::mode(mode) = set.mode {
            a.permissions = Some(mode);
        }
        if let set_size3::size(size) = set.size {
            a.size = Some(size);
        }
        let now = || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as u32)
                .unwrap_or(0)
        };
        let mtime = match set.mtime {
            set_mtime::SET_TO_CLIENT_TIME(t) => Some(t.seconds),
            set_mtime::SET_TO_SERVER_TIME => Some(now()),
            set_mtime::DONT_CHANGE => None,
        };
        let atime = match set.atime {
            set_atime::SET_TO_CLIENT_TIME(t) => Some(t.seconds),
            set_atime::SET_TO_SERVER_TIME => Some(now()),
            set_atime::DONT_CHANGE => None,
        };
        // SFTP v3 sets both times together; keep the one not being changed.
        if mtime.is_some() || atime.is_some() {
            let p = path.to_owned();
            let current = self
                .link
                .call(|s| async move { s.lstat(p).await })
                .await
                .map_err(nfs_error)?
                .attrs;
            a.mtime = mtime.or(current.mtime);
            a.atime = atime.or(current.atime);
        }
        if a.permissions.is_none() && a.size.is_none() && a.mtime.is_none() {
            return Ok(());
        }
        let p = path.to_owned();
        self.status(async move |s| s.setstat(p, a).await.map(drop))
            .await
    }

    async fn exists(&self, path: &str) -> bool {
        let p = path.to_owned();
        self.link
            .call(|s| async move { s.lstat(p).await })
            .await
            .is_ok()
    }

    /// Close one cached read handle so later reads see fresh content.
    async fn forget_handle(&self, id: fileid3) {
        let handle = self.handles.lock().unwrap().remove(&id);
        if let Some(h) = handle
            && h.generation == self.link.generation()
        {
            let _ = self
                .link
                .call(|s| async move { s.close(h.handle).await })
                .await;
        }
    }

    /// Close idle handles, and the oldest ones beyond the cap.
    async fn sweep_handles(&self) {
        let expired: Vec<fileid3> = {
            let handles = self.handles.lock().unwrap();
            let mut by_age: Vec<(fileid3, Instant)> =
                handles.iter().map(|(id, h)| (*id, h.used)).collect();
            by_age.sort_by_key(|(_, used)| *used);
            let surplus = by_age.len().saturating_sub(MAX_HANDLES);
            by_age
                .iter()
                .enumerate()
                .filter(|(i, (_, used))| *i < surplus || used.elapsed() >= HANDLE_TTL)
                .map(|(_, (id, _))| *id)
                .collect()
        };
        for id in expired {
            self.forget_handle(id).await;
        }
    }

    async fn read_handle(&self, id: fileid3, path: &str) -> Result<String, CallError> {
        self.sweep_handles().await;
        let generation = self.link.generation();
        if let Some(h) = self.handles.lock().unwrap().get_mut(&id)
            && h.generation == generation
        {
            h.used = Instant::now();
            return Ok(h.handle.clone());
        }
        let p = path.to_owned();
        let handle = self
            .link
            .call(|s| async move { s.open(p, OpenFlags::READ, FileAttributes::empty()).await })
            .await?
            .handle;
        self.handles.lock().unwrap().insert(
            id,
            Handle {
                handle: handle.clone(),
                generation: self.link.generation(),
                used: Instant::now(),
            },
        );
        Ok(handle)
    }

    async fn list(
        &self,
        dirid: fileid3,
        dir: &str,
    ) -> Result<Vec<(String, FileAttributes)>, nfsstat3> {
        if let Some(listing) = self.listings.lock().unwrap().get(&dirid)
            && listing.at.elapsed() < CACHE_TTL
        {
            return Ok(listing.entries.clone());
        }
        let mut entries = match self.list_once(dir).await {
            Err(CallError::Transport(_)) => self.list_once(dir).await,
            other => other,
        }
        .map_err(nfs_error)?;
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let now = Instant::now();
        {
            let mut ids = self.ids.lock().unwrap();
            let mut attrs = self.attrs.lock().unwrap();
            for (name, a) in &entries {
                attrs.insert(ids.id(&child(dir, name)), (now, a.clone()));
            }
        }
        let mut listings = self.listings.lock().unwrap();
        if listings.len() >= MAX_CACHED_LISTINGS {
            listings.retain(|_, l| l.at.elapsed() < CACHE_TTL);
        }
        listings.insert(
            dirid,
            Listing {
                at: now,
                entries: entries.clone(),
            },
        );
        Ok(entries)
    }
}

impl SftpFs {
    async fn list_once(&self, dir: &str) -> Result<Vec<(String, FileAttributes)>, CallError> {
        let path = self.abs(dir);
        let handle = self
            .link
            .call(|s| async move { s.opendir(path).await })
            .await?
            .handle;
        let mut entries = Vec::new();
        let outcome = loop {
            let h = handle.clone();
            match self.link.call(|s| async move { s.readdir(h).await }).await {
                Ok(name) => entries.extend(
                    name.files
                        .into_iter()
                        .filter(|f| f.filename != "." && f.filename != "..")
                        .map(|f| (f.filename, f.attrs)),
                ),
                Err(CallError::Status(StatusCode::Eof)) => break Ok(entries),
                Err(e) => break Err(e),
            }
        };
        let _ = self
            .link
            .call(|s| async move { s.close(handle).await })
            .await;
        outcome
    }

    /// One READ through the cached handle. A transport failure is returned
    /// as such so the caller can retry on a fresh session: reads are safe
    /// to repeat, unlike creations, removals or renames.
    async fn read_once(
        &self,
        id: fileid3,
        path: &str,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), CallError> {
        let handle = self.read_handle(id, path).await?;
        // Issue every chunk at once: SFTP pipelines requests by id.
        let reads = (0..(count as usize).div_ceil(CHUNK)).map(|i| {
            let h = handle.clone();
            let off = offset + (i * CHUNK) as u64;
            let len = (count as usize - i * CHUNK).min(CHUNK) as u32;
            self.link
                .call(move |s| async move { s.read(h, off, len).await })
        });
        let mut data = Vec::with_capacity(count as usize);
        let mut eof = false;
        for result in futures::future::join_all(reads).await {
            match result {
                Ok(_) if eof => {}
                Ok(chunk) => {
                    eof = chunk.data.len() < CHUNK;
                    data.extend_from_slice(&chunk.data);
                }
                Err(CallError::Status(StatusCode::Eof)) => eof = true,
                Err(e) => {
                    self.forget_handle(id).await;
                    return Err(e);
                }
            }
        }
        let eof = eof || data.len() < count as usize;
        Ok((data, eof))
    }
}

#[async_trait]
impl NFSFileSystem for SftpFs {
    fn capabilities(&self) -> VFSCapabilities {
        VFSCapabilities::ReadWrite
    }

    fn root_dir(&self) -> fileid3 {
        1
    }

    async fn lookup(&self, dirid: fileid3, filename: &filename3) -> Result<fileid3, nfsstat3> {
        let dir = self.rel(dirid)?;
        match filename.0.as_slice() {
            b"." => return Ok(dirid),
            b".." => return Ok(self.id(parent(&dir))),
            _ => {}
        }
        let rel = child(&dir, &component(filename)?);
        if is_local(&rel) {
            if !self.local.lock().unwrap().contains_key(&rel) {
                return Err(nfsstat3::NFS3ERR_NOENT);
            }
            return Ok(self.id(&rel));
        }
        let id = self.id(&rel);
        if let Err(e) = self.lstat(id, &rel).await {
            if matches!(e, nfsstat3::NFS3ERR_NOENT) {
                self.ids.lock().unwrap().forget(&rel);
            }
            return Err(e);
        }
        Ok(id)
    }

    async fn getattr(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        self.stat(id).await
    }

    async fn setattr(&self, id: fileid3, setattr: sattr3) -> Result<fattr3, nfsstat3> {
        let rel = self.rel(id)?;
        if is_local(&rel) {
            if let set_size3::size(n) = setattr.size
                && let Some(data) = self.local.lock().unwrap().get_mut(&rel)
            {
                data.resize(n as usize, 0);
            }
            return self.local_attr(id, &rel);
        }
        self.forget_handle(id).await;
        self.apply(&self.abs(&rel), &setattr).await?;
        self.changed(&rel);
        self.stat(id).await
    }

    async fn read(
        &self,
        id: fileid3,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), nfsstat3> {
        let rel = self.rel(id)?;
        if is_local(&rel) {
            let local = self.local.lock().unwrap();
            let data = local.get(&rel).ok_or(nfsstat3::NFS3ERR_NOENT)?;
            let start = (offset as usize).min(data.len());
            let end = start.saturating_add(count as usize).min(data.len());
            return Ok((data[start..end].to_vec(), end == data.len()));
        }
        let path = self.abs(&rel);
        let result = match self.read_once(id, &path, offset, count).await {
            Err(CallError::Transport(_)) => self.read_once(id, &path, offset, count).await,
            other => other,
        };
        result.map_err(nfs_error)
    }

    async fn write(&self, id: fileid3, offset: u64, data: &[u8]) -> Result<fattr3, nfsstat3> {
        let rel = self.rel(id)?;
        if is_local(&rel) {
            {
                let mut local = self.local.lock().unwrap();
                let buffer = local.get_mut(&rel).ok_or(nfsstat3::NFS3ERR_NOENT)?;
                let end = offset as usize + data.len();
                if buffer.len() < end {
                    buffer.resize(end, 0);
                }
                buffer[offset as usize..end].copy_from_slice(data);
            }
            return self.local_attr(id, &rel);
        }
        self.forget_handle(id).await;
        let path = self.abs(&rel);
        let handle = self
            .link
            .call(|s| async move {
                s.open(path, OpenFlags::WRITE, FileAttributes::empty())
                    .await
            })
            .await
            .map_err(nfs_error)?
            .handle;
        let writes = data.chunks(CHUNK).enumerate().map(|(i, part)| {
            let h = handle.clone();
            let part = part.to_vec();
            let off = offset + (i * CHUNK) as u64;
            self.link
                .call(move |s| async move { s.write(h, off, part).await })
        });
        let failed = futures::future::join_all(writes)
            .await
            .into_iter()
            .find_map(Result::err);
        let closed = self
            .link
            .call(|s| async move { s.close(handle).await })
            .await;
        self.changed(&rel);
        if let Some(e) = failed {
            return Err(nfs_error(e));
        }
        closed.map_err(nfs_error)?;
        self.stat(id).await
    }

    async fn create(
        &self,
        dirid: fileid3,
        filename: &filename3,
        attr: sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let rel = child(&self.rel(dirid)?, &component(filename)?);
        if is_local(&rel) {
            self.local.lock().unwrap().insert(rel.clone(), Vec::new());
            let id = self.id(&rel);
            return Ok((id, self.local_attr(id, &rel)?));
        }
        let path = self.abs(&rel);
        let p = path.clone();
        let flags = OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE;
        let handle = self
            .link
            .call(|s| async move { s.open(p, flags, FileAttributes::empty()).await })
            .await
            .map_err(nfs_error)?
            .handle;
        self.status(async move |s| s.close(handle).await.map(drop))
            .await?;
        let id = self.id(&rel);
        self.changed(&rel);
        self.forget_handle(id).await;
        self.apply(&path, &attr).await?;
        Ok((id, self.stat(id).await?))
    }

    async fn create_exclusive(
        &self,
        dirid: fileid3,
        filename: &filename3,
    ) -> Result<fileid3, nfsstat3> {
        let rel = child(&self.rel(dirid)?, &component(filename)?);
        if is_local(&rel) {
            let mut local = self.local.lock().unwrap();
            if local.contains_key(&rel) {
                return Err(nfsstat3::NFS3ERR_EXIST);
            }
            local.insert(rel.clone(), Vec::new());
            drop(local);
            return Ok(self.id(&rel));
        }
        let path = self.abs(&rel);
        let p = path.clone();
        let flags = OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE;
        match self
            .link
            .call(|s| async move { s.open(p, flags, FileAttributes::empty()).await })
            .await
        {
            Ok(h) => {
                self.status(async move |s| s.close(h.handle).await.map(drop))
                    .await?
            }
            // SFTP v3 reports an existing target only as a generic failure.
            Err(CallError::Status(StatusCode::Failure)) if self.exists(&path).await => {
                return Err(nfsstat3::NFS3ERR_EXIST);
            }
            Err(e) => return Err(nfs_error(e)),
        }
        let id = self.id(&rel);
        self.changed(&rel);
        Ok(id)
    }

    async fn mkdir(
        &self,
        dirid: fileid3,
        dirname: &filename3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let rel = child(&self.rel(dirid)?, &component(dirname)?);
        let path = self.abs(&rel);
        if self.exists(&path).await {
            return Err(nfsstat3::NFS3ERR_EXIST);
        }
        self.status(async move |s| s.mkdir(path, FileAttributes::empty()).await.map(drop))
            .await?;
        let id = self.id(&rel);
        self.changed(&rel);
        Ok((id, self.stat(id).await?))
    }

    async fn remove(&self, dirid: fileid3, filename: &filename3) -> Result<(), nfsstat3> {
        let rel = child(&self.rel(dirid)?, &component(filename)?);
        if is_local(&rel) {
            self.local
                .lock()
                .unwrap()
                .remove(&rel)
                .ok_or(nfsstat3::NFS3ERR_NOENT)?;
            self.ids.lock().unwrap().forget(&rel);
            return Ok(());
        }
        let id = self.id(&rel);
        self.forget_handle(id).await;
        self.attrs.lock().unwrap().remove(&id);
        let a = self.lstat(id, &rel).await?;
        let path = self.abs(&rel);
        if matches!(kind(&a), ftype3::NF3DIR) {
            self.link
                .call(|s| async move { s.rmdir(path).await })
                .await
                .map_err(|e| match e {
                    // OpenSSH reports ENOTEMPTY as a generic failure.
                    CallError::Status(StatusCode::Failure) => nfsstat3::NFS3ERR_NOTEMPTY,
                    other => nfs_error(other),
                })?;
        } else {
            self.status(async move |s| s.remove(path).await.map(drop))
                .await?;
        }
        self.changed(&rel);
        self.ids.lock().unwrap().forget(&rel);
        Ok(())
    }

    async fn rename(
        &self,
        from_dirid: fileid3,
        from_filename: &filename3,
        to_dirid: fileid3,
        to_filename: &filename3,
    ) -> Result<(), nfsstat3> {
        let from = child(&self.rel(from_dirid)?, &component(from_filename)?);
        let to = child(&self.rel(to_dirid)?, &component(to_filename)?);
        if is_local(&from) || is_local(&to) {
            if !(is_local(&from) && is_local(&to)) {
                return Err(nfsstat3::NFS3ERR_ACCES);
            }
            let mut local = self.local.lock().unwrap();
            let data = local.remove(&from).ok_or(nfsstat3::NFS3ERR_NOENT)?;
            local.insert(to.clone(), data);
            drop(local);
            self.ids.lock().unwrap().rename(&from, &to);
            return Ok(());
        }
        for rel in [&from, &to] {
            let id = self.ids.lock().unwrap().by_path.get(rel.as_str()).copied();
            if let Some(id) = id {
                self.forget_handle(id).await;
            }
            self.changed(rel);
        }
        // Plain SFTP v3 rename refuses an existing target, but editors save
        // by atomically replacing the file: use OpenSSH's posix-rename.
        let mut data = Vec::new();
        ssh_string(&self.abs(&from), &mut data);
        ssh_string(&self.abs(&to), &mut data);
        match self
            .link
            .call(|s| async move { s.extended("posix-rename@openssh.com", data).await })
            .await
            .map_err(nfs_error)?
        {
            Packet::Status(s) if s.status_code == StatusCode::Ok => {}
            Packet::Status(s) => return Err(nfs_error(CallError::Status(s.status_code))),
            _ => return Err(nfsstat3::NFS3ERR_IO),
        }
        {
            let mut ids = self.ids.lock().unwrap();
            ids.rename(&from, &to);
        }
        // Cached attributes of moved descendants are keyed by stable ids.
        self.changed(&from);
        self.changed(&to);
        Ok(())
    }

    async fn readdir(
        &self,
        dirid: fileid3,
        start_after: fileid3,
        max_entries: usize,
    ) -> Result<ReadDirResult, nfsstat3> {
        let dir = self.rel(dirid)?;
        let listing = self.list(dirid, &dir).await?;
        let mut entries: Vec<DirEntry> = {
            let mut ids = self.ids.lock().unwrap();
            listing
                .iter()
                .map(|(name, a)| {
                    let id = ids.id(&child(&dir, name));
                    DirEntry {
                        fileid: id,
                        name: name.as_bytes().into(),
                        attr: self.fattr(id, a),
                    }
                })
                .collect()
        };
        if start_after != 0 {
            let position = entries
                .iter()
                .position(|e| e.fileid == start_after)
                .ok_or(nfsstat3::NFS3ERR_BAD_COOKIE)?;
            entries.drain(..=position);
        }
        let end = entries.len() <= max_entries;
        entries.truncate(max_entries);
        Ok(ReadDirResult { entries, end })
    }

    async fn symlink(
        &self,
        dirid: fileid3,
        linkname: &filename3,
        symlink: &nfspath3,
        _attr: &sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let rel = child(&self.rel(dirid)?, &component(linkname)?);
        let target = std::str::from_utf8(&symlink.0)
            .map_err(|_| nfsstat3::NFS3ERR_INVAL)?
            .to_owned();
        let path = self.abs(&rel);
        // OpenSSH's sftp-server takes SYMLINK arguments in reverse order
        // (target first), which russh-sftp passes through unchanged.
        self.status(async move |s| s.symlink(target, path).await.map(drop))
            .await?;
        let id = self.id(&rel);
        self.changed(&rel);
        Ok((id, self.stat(id).await?))
    }

    async fn readlink(&self, id: fileid3) -> Result<nfspath3, nfsstat3> {
        let path = self.abs(&self.rel(id)?);
        let name = self
            .link
            .call(|s| async move { s.readlink(path).await })
            .await
            .map_err(nfs_error)?;
        let file = name.files.into_iter().next().ok_or(nfsstat3::NFS3ERR_IO)?;
        Ok(file.filename.into_bytes().into())
    }

    async fn fsinfo(&self, root_fileid: fileid3) -> Result<fsinfo3, nfsstat3> {
        let attributes = match self.getattr(root_fileid).await {
            Ok(a) => post_op_attr::attributes(a),
            Err(_) => post_op_attr::Void,
        };
        Ok(fsinfo3 {
            obj_attributes: attributes,
            rtmax: TRANSFER,
            rtpref: TRANSFER,
            rtmult: 4096,
            wtmax: TRANSFER,
            wtpref: TRANSFER,
            wtmult: 4096,
            dtpref: 64 * 1024,
            maxfilesize: u64::MAX,
            time_delta: nfstime3 {
                seconds: 1,
                nseconds: 0,
            },
            properties: FSF_SYMLINK | FSF_HOMOGENEOUS | FSF_CANSETTIME,
        })
    }
}

/// The NFS export name and mount source for a workspace, e.g. `127.0.0.1:/demo`.
pub fn mount_source(workspace: &str) -> String {
    format!("127.0.0.1:/{workspace}")
}

/// Default mount point for the NFS backend: a user-owned directory, since
/// creating entries under `/Volumes` requires root.
pub fn default_mount_root(home: &Path, workspace: &str) -> PathBuf {
    home.join("RWS").join(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_files_stay_local() {
        assert!(is_local("._notes.txt"));
        assert!(is_local("dir/._x"));
        assert!(is_local("dir/.DS_Store"));
        assert!(!is_local("dir/notes.txt"));
        assert!(!is_local(".gitignore"));
    }

    #[test]
    fn names_are_composed_and_validated() {
        let decomposed = "re\u{301}sume\u{301}.txt";
        assert_eq!(
            component(&decomposed.as_bytes().into()).unwrap(),
            "résumé.txt"
        );
        for bad in ["", ".", "..", "a/b", "nul\0"] {
            assert!(component(&bad.as_bytes().into()).is_err(), "{bad:?}");
        }
        assert!(component(&vec![0xff, 0xfe].into()).is_err());
    }

    #[test]
    fn rename_moves_descendants_and_replaces_target() {
        let mut ids = Ids::new();
        let dir = ids.id("a");
        let file = ids.id("a/b/c.txt");
        let target = ids.id("z");
        ids.rename("a", "z");
        assert_eq!(ids.paths[&dir], "z");
        assert_eq!(ids.paths[&file], "z/b/c.txt");
        assert!(!ids.paths.contains_key(&target));
        assert_eq!(ids.by_path["z/b/c.txt"], file);
        assert!(!ids.by_path.contains_key("a/b/c.txt"));
        // A sibling sharing the prefix must not move.
        let sibling = ids.id("zz");
        ids.rename("z", "y");
        assert_eq!(ids.paths[&sibling], "zz");
    }

    #[test]
    fn paths_join_under_any_root() {
        let fs = SftpFs::new(Link::new("h"), "/".into(), 0, 0);
        assert_eq!(fs.abs(""), "/");
        assert_eq!(fs.abs("a/b"), "/a/b");
        let fs = SftpFs {
            root: "/srv/x".into(),
            ..fs
        };
        assert_eq!(fs.abs("a"), "/srv/x/a");
        assert_eq!(parent("a/b/c"), "a/b");
        assert_eq!(parent("a"), "");
    }

    #[test]
    fn mount_options_bound_the_kernel_timeout_above_the_bridge() {
        let options = mount_options(4242);
        assert!(options.contains("port=4242,mountport=4242"));
        assert!(options.contains("soft"));
        let value = |key: &str| -> u64 {
            options
                .split(',')
                .find_map(|o| o.strip_prefix(key))
                .unwrap()
                .parse()
                .unwrap()
        };
        // A soft mount fails after `retrans` intervals of `timeo` tenths of a
        // second. Together they must outlast the bridge's own deadline, so
        // the kernel never gives up on a bridge that is still answering.
        assert!(
            Duration::from_millis(value("timeo=") * 100) * value("retrans=") as u32
                > OPERATION_TIMEOUT
        );
        assert!(value("retrans=") >= 2);
    }

    #[test]
    fn nfs_mounts_default_under_the_home_directory() {
        assert_eq!(
            default_mount_root(Path::new("/Users/me"), "demo"),
            PathBuf::from("/Users/me/RWS/demo")
        );
        assert_eq!(mount_source("demo"), "127.0.0.1:/demo");
    }
}
