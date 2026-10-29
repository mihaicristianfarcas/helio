//! Updating from a fake release: a server on this computer stands in for
//! GitHub (`HELIO_UPDATE_API`), its latest release a version newer than the
//! build's, with this system's package (made here, named as the packaging
//! scripts name it), and the updater downloads it from there, checks it
//! and puts it in place as an update would (`install::rehearse`: into a
//! folder of the test's; nothing quits, opens or restarts).
//!
//! How a package is checked and put in place is each system's own. Here
//! the Linux archive is unpacked into place; the Windows installer is
//! copied (its signature is required only by a build given a publisher,
//! which a test's build is not; running it is a manual check, in
//! docs/development.md); and a Mac bundle signed ad hoc, as a test can sign
//! one, is refused: an update must carry the same team's Developer ID as
//! the app it replaces. With a Developer ID identity in the keychain, the
//! ignored test puts a signed one in place:
//!
//!   HELIO_TEST_DEVELOPER_ID="Developer ID Application: …" \
//!     cargo test -p helio-update --test fake_release -- --ignored

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};

use helio_update::install::{package, rehearse};
use helio_update::{Build, REPOSITORY};
use sha2::{Digest, Sha256};

/// `HELIO_UPDATE_API` is the process's: one test at a time sets it.
static API: Mutex<()> = Mutex::new(());

/// The build that updates.
const MINE: Build = Build {
    version: "1.0.0",
    commit: "",
    release: true,
};

/// The fake release.
const NEWER: &str = "1.0.1";

fn slug() -> &'static str {
    REPOSITORY
        .strip_prefix("https://github.com/")
        .expect("the repository is on GitHub")
}

fn sha256(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A server on this computer, and what it has been asked.
struct Server {
    base: String,
    listener: Option<TcpListener>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn bind() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        Server {
            base: format!("http://{}", listener.local_addr().unwrap()),
            listener: Some(listener),
            asked: Arc::default(),
        }
    }

    /// Answer GET for each path given, 404 for any other, from now on.
    fn serve(&mut self, files: Vec<(String, Vec<u8>)>) {
        let listener = self.listener.take().expect("served once");
        let asked = self.asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    continue;
                }
                // The headers, up to the blank line.
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                    line.clear();
                }
                let path = request.split(' ').nth(1).unwrap_or("").to_string();
                asked.lock().unwrap().push(path.clone());
                let (status, body): (&str, &[u8]) = match files.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => ("200 OK", body),
                    None => ("404 Not Found", b"{\"message\": \"Not Found\"}"),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body);
            }
        });
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

/// Where GitHub's API has the latest release.
fn latest_path() -> String {
    format!("/repos/{}/releases/latest", slug())
}

/// Where GitHub's site has a file of the fake release.
fn download_path(name: &str) -> String {
    format!("/{}/releases/download/v{NEWER}/{name}", slug())
}

/// GitHub's latest release, as its API writes it: this system's package,
/// at `url`, listed with `listed`'s size and SHA-256.
fn release(url: &str, listed: &[u8]) -> Vec<u8> {
    serde_json::json!({
        "tag_name": format!("v{NEWER}"),
        "html_url": format!("https://github.com/{}/releases/tag/v{NEWER}", slug()),
        "assets": [{
            "name": package(NEWER),
            "size": listed.len(),
            "browser_download_url": url,
            "digest": format!("sha256:{}", sha256(listed)),
        }],
    })
    .to_string()
    .into_bytes()
}

/// The fake release, served with its package by one server that the
/// updater is pointed at: the package `served` where the release says it
/// is, listed as `listed`. The guard keeps other tests from pointing the
/// updater elsewhere meanwhile.
fn fake_release(listed: &[u8], served: &[u8]) -> (Server, MutexGuard<'static, ()>) {
    let guard = API.lock().unwrap_or_else(|e| e.into_inner());
    let mut server = Server::bind();
    let download = download_path(&package(NEWER));
    let url = format!("{}{download}", server.base);
    server.serve(vec![
        (latest_path(), release(&url, listed)),
        (download, served.to_vec()),
    ]);
    std::env::set_var("HELIO_UPDATE_API", &server.base);
    (server, guard)
}

fn run(cmd: &mut Command) {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// This system's package of the fake release, made in `dir` as the
/// packaging scripts make it, with `mark` inside to tell it apart.
fn make_package(dir: &Path, mark: &str) -> Vec<u8> {
    if cfg!(target_os = "linux") {
        // tools/linux/package: one folder, the programs and install.sh.
        let name = format!("Helio-{NEWER}-linux-{}", std::env::consts::ARCH);
        let folder = dir.join(&name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("install.sh"), "#!/bin/sh\necho installed\n").unwrap();
        std::fs::write(folder.join("helio-app"), mark).unwrap();
        let archive = dir.join("package.tar.gz");
        run(Command::new("tar")
            .arg("-C")
            .arg(dir)
            .arg("-czf")
            .arg(&archive)
            .arg(&name));
        std::fs::read(archive).unwrap()
    } else if cfg!(target_os = "macos") {
        // tools/package-macos: Helio.app, zipped by ditto.
        let app = mac_bundle(dir, mark, None);
        let archive = dir.join("package.zip");
        run(Command::new("/usr/bin/ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(&app)
            .arg(&archive));
        std::fs::read(archive).unwrap()
    } else {
        // tools/package-windows.ps1's installer: a program, as far as the
        // updater looks before running it.
        format!("MZ a stand-in for Helio's installer ({mark})").into_bytes()
    }
}

/// A Helio.app in `dir` whose program says `mark`, signed by `identity`,
/// or ad hoc without one.
fn mac_bundle(dir: &Path, mark: &str, identity: Option<&str>) -> std::path::PathBuf {
    let app = dir.join("Helio.app");
    let _ = std::fs::remove_dir_all(&app);
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::write(
        app.join("Contents/Info.plist"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\
         <key>CFBundleIdentifier</key><string>dev.mihaicristian.helio.test</string>\
         <key>CFBundleExecutable</key><string>Helio</string>\
         <key>CFBundlePackageType</key><string>APPL</string>\
         </dict></plist>\n",
    )
    .unwrap();
    let program = app.join("Contents/MacOS/Helio");
    std::fs::write(&program, format!("#!/bin/sh\n# {mark}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    run(Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", identity.unwrap_or("-")])
        .arg(&app));
    app
}

/// What of the package `rehearse` put in `folder`, for comparing with its
/// mark: the program inside it.
fn installed(folder: &Path) -> Option<String> {
    let file = if cfg!(target_os = "linux") {
        folder
            .join(format!("Helio-{NEWER}-linux-{}", std::env::consts::ARCH))
            .join("helio-app")
    } else if cfg!(target_os = "macos") {
        folder.join("Helio.app/Contents/MacOS/Helio")
    } else {
        folder.join(package(NEWER))
    };
    std::fs::read_to_string(file).ok()
}

#[test]
fn a_build_takes_the_newer_release_from_the_server_it_is_pointed_at() {
    let dir = tempfile::tempdir().unwrap();
    let data = make_package(dir.path(), "release 1.0.1");
    let (server, _guard) = fake_release(&data, &data);
    let folder = dir.path().join("installed");
    if cfg!(target_os = "macos") {
        // The app there is signed ad hoc, as is the release's: neither is a
        // Developer ID's, and nothing is put in place.
        std::fs::create_dir_all(&folder).unwrap();
        mac_bundle(&folder, "release 1.0.0", None);
        let err = rehearse(&MINE, &folder, &|_, _| {}).unwrap_err();
        assert!(err.contains("not signed with a Developer ID"), "{err}");
        assert!(installed(&folder).unwrap().contains("release 1.0.0"));
    } else {
        assert_eq!(rehearse(&MINE, &folder, &|_, _| {}), Ok(NEWER.into()));
        assert!(installed(&folder).unwrap().contains("release 1.0.1"));
    }
    // Asked of the fake, not of GitHub: the release, then its package.
    assert_eq!(
        server.asked(),
        [latest_path(), download_path(&package(NEWER))]
    );
}

#[test]
fn a_package_that_is_not_the_file_the_release_lists_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let data = make_package(dir.path(), "release 1.0.1");
    // The same size, other bytes: what a changed download would be.
    let mut tampered = data.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xff;
    let (_server, _guard) = fake_release(&data, &tampered);
    let folder = dir.path().join("installed");
    let err = rehearse(&MINE, &folder, &|_, _| {}).unwrap_err();
    assert!(err.contains("SHA-256 differs"), "{err}");
    assert_eq!(installed(&folder), None);
}

#[test]
fn a_build_of_the_latest_release_has_nothing_to_install() {
    let dir = tempfile::tempdir().unwrap();
    let data = make_package(dir.path(), "release 1.0.1");
    let (server, _guard) = fake_release(&data, &data);
    let same = Build {
        version: NEWER,
        ..MINE
    };
    let err = rehearse(&same, &dir.path().join("installed"), &|_, _| {}).unwrap_err();
    assert!(err.contains("is the newest release"), "{err}");
    // Nothing downloaded.
    assert_eq!(server.asked(), [latest_path()]);
}

#[test]
fn a_package_served_from_another_server_is_not_taken() {
    let dir = tempfile::tempdir().unwrap();
    let data = make_package(dir.path(), "release 1.0.1");
    let _guard = API.lock().unwrap_or_else(|e| e.into_inner());
    // The release, on the server the updater is pointed at, names a file
    // on another.
    let mut elsewhere = Server::bind();
    let download = download_path(&package(NEWER));
    let url = format!("{}{download}", elsewhere.base);
    elsewhere.serve(vec![(download, data.clone())]);
    let mut api = Server::bind();
    api.serve(vec![(latest_path(), release(&url, &data))]);
    std::env::set_var("HELIO_UPDATE_API", &api.base);
    let err = rehearse(&MINE, &dir.path().join("installed"), &|_, _| {}).unwrap_err();
    assert!(err.contains("with a checksum"), "{err}");
    assert!(elsewhere.asked().is_empty());
}

#[test]
#[ignore = "requires a Developer ID identity in the keychain: HELIO_TEST_DEVELOPER_ID"]
fn a_mac_takes_a_release_signed_by_its_own_team() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let identity = std::env::var("HELIO_TEST_DEVELOPER_ID")
        .expect("HELIO_TEST_DEVELOPER_ID: a Developer ID Application identity");
    let dir = tempfile::tempdir().unwrap();
    let new = dir.path().join("new");
    std::fs::create_dir_all(&new).unwrap();
    mac_bundle(&new, "release 1.0.1", Some(&identity));
    let archive = dir.path().join("package.zip");
    run(Command::new("/usr/bin/ditto")
        .args(["-c", "-k", "--keepParent"])
        .arg(new.join("Helio.app"))
        .arg(&archive));
    let data = std::fs::read(&archive).unwrap();
    let folder = dir.path().join("installed");
    std::fs::create_dir_all(&folder).unwrap();
    mac_bundle(&folder, "release 1.0.0", Some(&identity));
    {
        let (_server, _guard) = fake_release(&data, &data);
        assert_eq!(rehearse(&MINE, &folder, &|_, _| {}), Ok(NEWER.into()));
        assert!(installed(&folder).unwrap().contains("release 1.0.1"));
    }
    // And one signed by another team, or by none, is not.
    mac_bundle(&new, "release 1.0.1, signed ad hoc", None);
    let _ = std::fs::remove_file(&archive);
    run(Command::new("/usr/bin/ditto")
        .args(["-c", "-k", "--keepParent"])
        .arg(new.join("Helio.app"))
        .arg(&archive));
    let ad_hoc = std::fs::read(&archive).unwrap();
    let (_server, _guard) = fake_release(&ad_hoc, &ad_hoc);
    let err = rehearse(&MINE, &folder, &|_, _| {}).unwrap_err();
    assert!(err.contains("is not signed by this app's maker"), "{err}");
    assert!(installed(&folder).unwrap().contains("release 1.0.1"));
}
