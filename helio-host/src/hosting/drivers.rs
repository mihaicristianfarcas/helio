//! The drivers Helio Host uses on Windows, installed as hosting is turned
//! on and removed as it is turned off: SudoVDA, for a virtual display at
//! each client's mode, and ViGEmBus, for its controllers. Apollo's
//! installer installs the same two (its `windows_nsis.cmake` runs
//! SudoVDA's `install.bat` and `install-gamepad.ps1`); here `enable` does,
//! which runs as an administrator anyway, so a PC that only plays never
//! gets either.
//!
//! - **SudoVDA** comes inside this program (`vendor/sudovda`, whose README
//!   says where the files are from): its package is written into the
//!   engine's folder, its publisher's certificate trusted on the machine
//!   (a user-mode driver signed by its author installs only so), a
//!   root-enumerated device made with its hardware ID, and the driver
//!   installed on it, as Apollo's `nefconc` calls do, here with SetupAPI. It
//!   is built for x64 PCs only.
//! - **ViGEmBus** is its author's signed installer, shipped beside
//!   `helio-host.exe` (`drivers\ViGEmBus_setup.exe`), copied into the
//!   engine's folder, checked against [`VIGEMBUS_SHA256`] there (where no
//!   user can change it) and run without a window.
//!
//! A driver already here (Apollo's SudoVDA, ViGEmBus from another program)
//! is used as it is, and left when hosting is turned off: only what
//! `enable` installed goes, as `drivers\installed.toml` in the engine's
//! folder records. Neither driver is needed to host — without SudoVDA a
//! session streams the main display, without ViGEmBus it takes no
//! controllers — so one that does not install is a line in what `enable`
//! says, not a failure.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows::core::BOOL;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::*;
use windows::Win32::Devices::Properties::{DEVPKEY_Device_DriverInfPath, DEVPROPTYPE};
use windows::Win32::Foundation::{CRYPT_E_EXISTS, HWND};
use windows::Win32::Security::Cryptography::*;

/// SudoVDA's package (`vendor/sudovda/README.md`).
const SUDOVDA_INF: &[u8] = include_bytes!("../../../vendor/sudovda/SudoVDA.inf");
const SUDOVDA_CAT: &[u8] = include_bytes!("../../../vendor/sudovda/sudovda.cat");
const SUDOVDA_DLL: &[u8] = include_bytes!("../../../vendor/sudovda/SudoVDA.dll");
/// `CN=sudovda@su.mk`, who signed the package.
const SUDOVDA_CERT: &[u8] = include_bytes!("../../../vendor/sudovda/sudovda.cer");
/// The hardware ID its INF installs on, which its root device is made
/// with.
const SUDOVDA_HWID: &str = r"Root\SudoMaker\SudoVDA";

/// `ViGEmBus_1.22.0_x64_x86_arm64.exe`, from github.com/nefarius/ViGEmBus
/// (release v1.22.0, BSD-3-Clause): what `tools/package-windows.ps1`
/// downloads, with the same hash.
pub const VIGEMBUS_SETUP: &str = "ViGEmBus_setup.exe";
const VIGEMBUS_SHA256: &str = "89220a7865076b342892f98865f3499fb7c4cfd673159e89d352c360fd014c6a";
/// What it installs: `System32\drivers\ViGEmBus.sys`, as Sunshine looks
/// for it (`confighttp.cpp`), and its Windows Installer product (the
/// setup's `DefaultProdCode`), which uninstalls it.
const VIGEMBUS_DRIVER: &str = r"drivers\ViGEmBus.sys";
const VIGEMBUS_PRODUCT: &str = "{966606F3-2745-49E9-BF15-5C3EAA4E9077}";

/// How long a driver may take to start once installed.
const START_WAIT: Duration = Duration::from_secs(10);

/// What `enable` installed, so that `disable` removes that and no more.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Installed {
    /// SudoVDA's root device, made here.
    #[serde(default)]
    sudovda_device: bool,
    /// The driver package as Windows' driver store named it (`oem42.inf`).
    #[serde(default)]
    sudovda_package: Option<String>,
    /// The stores its certificate was added to (`Root`, `TrustedPublisher`).
    #[serde(default)]
    sudovda_stores: Vec<String>,
    #[serde(default)]
    vigembus: bool,
}

impl Installed {
    fn path(dir: &Path) -> PathBuf {
        dir.join("drivers").join("installed.toml")
    }

    fn read(dir: &Path) -> Installed {
        std::fs::read_to_string(Self::path(dir))
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn write(&self, dir: &Path) {
        if let Ok(s) = toml::to_string(self) {
            let _ = std::fs::write(Self::path(dir), s);
        }
    }

    fn any(&self) -> bool {
        self.sudovda_device
            || self.sudovda_package.is_some()
            || !self.sudovda_stores.is_empty()
            || self.vigembus
    }
}

/// Install what is missing, into the engine's folder `dir`, ViGEmBus's
/// installer taken from `source` (beside the `helio-host.exe` that runs
/// `enable`); a line each, for what `enable` says.
pub fn install(dir: &Path, source: &Path) -> Vec<String> {
    let folder = dir.join("drivers");
    if let Err(e) = std::fs::create_dir_all(&folder).and_then(|()| {
        super::system::secure(&folder, super::system::FOLDER).map_err(std::io::Error::other)
    }) {
        return vec![format!("No drivers installed: {}: {e}", folder.display())];
    }
    let mut installed = Installed::read(dir);
    let lines = vec![
        sudovda(&folder, &mut installed).unwrap_or_else(|e| {
            format!("SudoVDA did not install ({e}): sessions stream the main display.")
        }),
        vigembus(&folder, source, &mut installed).unwrap_or_else(|e| {
            format!("ViGEmBus did not install ({e}): sessions take no controllers.")
        }),
    ];
    installed.write(dir);
    lines
}

/// Remove what [`install`] installed; a line each, for what `disable`
/// says.
pub fn remove(dir: &Path) -> Vec<String> {
    let installed = Installed::read(dir);
    let mut lines = Vec::new();
    if installed.sudovda_device || installed.sudovda_package.is_some() {
        lines.push(match remove_sudovda(&installed) {
            Ok(()) => "SudoVDA, which hosting installed, is removed.".to_string(),
            Err(e) => format!("SudoVDA is not removed: {e}"),
        });
    }
    for store in &installed.sudovda_stores {
        if let Err(e) = untrust(store) {
            lines.push(format!(
                "SudoVDA's certificate stays in the {store} store: {e}"
            ));
        }
    }
    if installed.vigembus {
        lines.push(match uninstall_product(VIGEMBUS_PRODUCT) {
            Ok(()) => "ViGEmBus, which hosting installed, is removed.".to_string(),
            Err(e) => format!("ViGEmBus is not removed: {e}"),
        });
    }
    if installed.any() {
        let _ = std::fs::remove_file(Installed::path(dir));
    }
    lines
}

fn sudovda(folder: &Path, installed: &mut Installed) -> Result<String, String> {
    if helio_display::sudovda::present() {
        return Ok(if installed.sudovda_device {
            "SudoVDA is installed: each session gets a virtual display.".into()
        } else {
            "SudoVDA was here already: each session gets a virtual display.".into()
        });
    }
    if !native_x64() {
        return Ok(
            "No SudoVDA: it is built for x64 PCs only, so sessions stream the main display.".into(),
        );
    }
    let package = folder.join("sudovda");
    std::fs::create_dir_all(&package).map_err(|e| e.to_string())?;
    for (name, bytes) in [
        ("SudoVDA.inf", SUDOVDA_INF),
        ("sudovda.cat", SUDOVDA_CAT),
        ("SudoVDA.dll", SUDOVDA_DLL),
    ] {
        std::fs::write(package.join(name), bytes)
            .map_err(|e| format!("{}: {e}", package.join(name).display()))?;
    }
    for store in ["Root", "TrustedPublisher"] {
        if trust(store)? {
            installed.sudovda_stores.push(store.to_string());
        }
    }
    if devices(SUDOVDA_HWID)?.is_empty() {
        make_device(SUDOVDA_HWID)?;
        installed.sudovda_device = true;
    }
    let inf = package.join("SudoVDA.inf");
    let mut reboot = BOOL(0);
    // SAFETY: both strings are NUL-terminated and live for the call; the
    // flag is written by the call.
    unsafe {
        UpdateDriverForPlugAndPlayDevicesW(
            None::<HWND>,
            &HSTRING::from(SUDOVDA_HWID),
            &HSTRING::from(inf.as_os_str()),
            INSTALLFLAG_FORCE,
            Some(&mut reboot),
        )
    }
    .map_err(|e| format!("installing its driver: {}", e.message()))?;
    installed.sudovda_package = devices(SUDOVDA_HWID)?.into_iter().find_map(|d| d.package);
    let deadline = Instant::now() + START_WAIT;
    while !helio_display::sudovda::present() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(if helio_display::sudovda::present() {
        "SudoVDA is installed: each session gets a virtual display.".into()
    } else if reboot.as_bool() {
        "SudoVDA is installed, and starts once Windows restarts.".into()
    } else {
        "SudoVDA is installed but has not started: sessions stream the main display until \
            it does."
            .into()
    })
}

fn vigembus(folder: &Path, source: &Path, installed: &mut Installed) -> Result<String, String> {
    if system_dir()?.join(VIGEMBUS_DRIVER).is_file() {
        return Ok(if installed.vigembus {
            "ViGEmBus is installed: sessions take controllers.".into()
        } else {
            "ViGEmBus was here already: sessions take controllers.".into()
        });
    }
    let from = source.join("drivers").join(VIGEMBUS_SETUP);
    let setup = folder.join(VIGEMBUS_SETUP);
    if from != setup {
        if !from.is_file() {
            return Ok(
                "No ViGEmBus: this build of Helio does not come with it, so sessions \
                take no controllers."
                    .into(),
            );
        }
        std::fs::copy(&from, &setup).map_err(|e| format!("{}: {e}", from.display()))?;
        super::system::secure(&setup, super::system::FILE)?;
    }
    let bytes = std::fs::read(&setup).map_err(|e| format!("{}: {e}", setup.display()))?;
    if !matches_sha256(&bytes, VIGEMBUS_SHA256) {
        let _ = std::fs::remove_file(&setup);
        return Err("its installer is not the one Helio ships".into());
    }
    // An Advanced Installer setup: `/exenoui` runs it without its own
    // window, and `/qn /norestart` go on to Windows Installer. 3010:
    // installed, a restart to finish.
    let status = std::process::Command::new(&setup)
        .args(["/exenoui", "/qn", "/norestart"])
        .status()
        .map_err(|e| format!("{}: {e}", setup.display()))?;
    match status.code() {
        Some(0) | Some(3010) => {}
        other => return Err(format!("its installer ended with {other:?}")),
    }
    installed.vigembus = true;
    Ok(if status.code() == Some(3010) {
        "ViGEmBus is installed, and takes controllers once Windows restarts.".into()
    } else {
        "ViGEmBus is installed: sessions take controllers.".into()
    })
}

fn matches_sha256(bytes: &[u8], hex: &str) -> bool {
    let digest = Sha256::digest(bytes);
    let got: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    got.eq_ignore_ascii_case(hex)
}

/// Whether Windows runs on an x64 processor (not Arm, where this program
/// runs emulated).
fn native_x64() -> bool {
    use windows::Win32::System::SystemInformation::{IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64};
    use windows::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};
    let (mut process, mut native) = (IMAGE_FILE_MACHINE(0), IMAGE_FILE_MACHINE(0));
    // SAFETY: the pseudo-handle needs no closing; both outputs are locals.
    unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, Some(&mut native)) }.is_ok()
        && native == IMAGE_FILE_MACHINE_AMD64
}

/// `C:\Windows\System32`, from the system rather than `%SystemRoot%`: an
/// elevated process keeps the environment of whoever started it.
fn system_dir() -> Result<PathBuf, String> {
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buf = [0u16; 260];
    // SAFETY: the buffer is a local of the length given.
    let n = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if n == 0 || n > buf.len() {
        return Err("cannot find the system folder".into());
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

/// Run `msiexec /x PRODUCT` without a window.
fn uninstall_product(product: &str) -> Result<(), String> {
    let status = std::process::Command::new(system_dir()?.join("msiexec.exe"))
        .args(["/x", product, "/qn", "/norestart"])
        .status()
        .map_err(|e| e.to_string())?;
    match status.code() {
        // 1605: not installed (someone removed it already).
        Some(0) | Some(3010) | Some(1605) => Ok(()),
        other => Err(format!("its uninstall ended with {other:?}")),
    }
}

/// The machine's certificate store `name`, opened for changes.
struct Store(HCERTSTORE);

impl Store {
    fn open(name: &str) -> Result<Store, String> {
        // SAFETY: the store name is NUL-terminated and lives for the call.
        unsafe {
            CertOpenStore(
                CERT_STORE_PROV_SYSTEM_W,
                CERT_QUERY_ENCODING_TYPE(0),
                None,
                CERT_OPEN_STORE_FLAGS(CERT_SYSTEM_STORE_LOCAL_MACHINE),
                Some(HSTRING::from(name).as_ptr() as *const _),
            )
        }
        .map(Store)
        .map_err(|e| format!("the {name} certificate store: {}", e.message()))
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: opened by `open`, closed once.
        let _ = unsafe { CertCloseStore(Some(self.0), 0) };
    }
}

/// Add SudoVDA's certificate to the machine's store `name`: whether it
/// was added (not there before).
fn trust(name: &str) -> Result<bool, String> {
    let store = Store::open(name)?;
    // SAFETY: the certificate's bytes are a constant; no context is
    // asked back.
    match unsafe {
        CertAddEncodedCertificateToStore(
            Some(store.0),
            X509_ASN_ENCODING,
            SUDOVDA_CERT,
            CERT_STORE_ADD_NEW,
            None,
        )
    } {
        Ok(()) => Ok(true),
        Err(e) if e.code() == CRYPT_E_EXISTS => Ok(false),
        Err(e) => Err(format!(
            "trusting its publisher ({name} store): {}",
            e.message()
        )),
    }
}

/// Take SudoVDA's certificate out of the machine's store `name` again.
fn untrust(name: &str) -> Result<(), String> {
    let store = Store::open(name)?;
    let thumbprint = Sha1::thumbprint(SUDOVDA_CERT);
    let blob = CRYPT_INTEGER_BLOB {
        cbData: thumbprint.len() as u32,
        pbData: thumbprint.as_ptr() as *mut u8,
    };
    // SAFETY: the blob points at `thumbprint`, alive for the call; the
    // context found is freed by the delete, which takes it.
    unsafe {
        let found = CertFindCertificateInStore(
            store.0,
            X509_ASN_ENCODING,
            0,
            CERT_FIND_SHA1_HASH,
            Some(&blob as *const _ as *const _),
            None,
        );
        if found.is_null() {
            return Ok(());
        }
        CertDeleteCertificateFromStore(found).map_err(|e| e.message())
    }
}

/// A certificate's SHA-1 thumbprint, which the stores find it by.
struct Sha1;

impl Sha1 {
    fn thumbprint(cert: &[u8]) -> Vec<u8> {
        let mut hash = vec![0u8; 20];
        let mut len = hash.len() as u32;
        // SAFETY: the output buffer and its length are locals; the call
        // writes no more than `len` bytes.
        let ok = unsafe {
            CryptHashCertificate(None, CALG_SHA1, 0, cert, Some(hash.as_mut_ptr()), &mut len)
        };
        if ok.is_ok() {
            hash.truncate(len as usize);
        }
        hash
    }
}

/// A device with a hardware ID: its instance, and the driver package it
/// runs (`oem42.inf`), if any.
struct Device {
    package: Option<String>,
}

/// The devices of the display class with `hwid`.
fn devices(hwid: &str) -> Result<Vec<Device>, String> {
    let mut found = Vec::new();
    each_device(hwid, |set, data| {
        found.push(Device {
            package: inf_path(set, data),
        });
        false
    })?;
    Ok(found)
}

/// Call `f` on each display-class device with `hwid` until it says to
/// stop.
fn each_device(
    hwid: &str,
    mut f: impl FnMut(HDEVINFO, &SP_DEVINFO_DATA) -> bool,
) -> Result<(), String> {
    // SAFETY: the list is destroyed before returning; each device's data
    // is a local the calls fill.
    unsafe {
        let set = SetupDiGetClassDevsW(
            Some(&GUID_DEVCLASS_DISPLAY),
            PCWSTR::null(),
            None,
            SETUP_DI_GET_CLASS_DEVS_FLAGS(0),
        )
        .map_err(|e| format!("listing display devices: {}", e.message()))?;
        let mut i = 0;
        loop {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(set, i, &mut data).is_err() {
                break;
            }
            i += 1;
            if hardware_ids(set, &data)
                .iter()
                .any(|id| id.eq_ignore_ascii_case(hwid))
                && f(set, &data)
            {
                break;
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    Ok(())
}

/// A device's hardware IDs (a `REG_MULTI_SZ`).
fn hardware_ids(set: HDEVINFO, data: &SP_DEVINFO_DATA) -> Vec<String> {
    let mut buf = [0u8; 1024];
    // SAFETY: the buffer is a local of the length given.
    let read = unsafe {
        SetupDiGetDeviceRegistryPropertyW(set, data, SPDRP_HARDWAREID, None, Some(&mut buf), None)
    };
    if read.is_err() {
        return Vec::new();
    }
    multi_sz(&buf)
}

/// The file name in the driver store of the package `data` runs.
fn inf_path(set: HDEVINFO, data: &SP_DEVINFO_DATA) -> Option<String> {
    let mut buf = [0u8; 520];
    let mut kind = DEVPROPTYPE(0);
    // SAFETY: the buffer and type are locals of the lengths given.
    unsafe {
        SetupDiGetDevicePropertyW(
            set,
            data,
            &DEVPKEY_Device_DriverInfPath,
            &mut kind,
            Some(&mut buf),
            None,
            0,
        )
    }
    .ok()?;
    multi_sz(&buf).into_iter().next()
}

/// The strings of a UTF-16 multi-string (or one string), from its bytes.
fn multi_sz(bytes: &[u8]) -> Vec<String> {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    units
        .split(|&u| u == 0)
        .take_while(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// A root-enumerated display device with `hwid`, for a driver to be
/// installed on (what `nefconc --create-device-node` and devcon's
/// `install` do).
fn make_device(hwid: &str) -> Result<(), String> {
    // The hardware ID as a REG_MULTI_SZ: the string, then two NULs.
    let mut ids: Vec<u16> = hwid.encode_utf16().collect();
    ids.extend([0, 0]);
    let bytes: Vec<u8> = ids.iter().flat_map(|u| u.to_le_bytes()).collect();
    // SAFETY: the list is destroyed before returning; the device's data is
    // a local the calls fill; the strings live for each call.
    unsafe {
        let set = SetupDiCreateDeviceInfoList(Some(&GUID_DEVCLASS_DISPLAY), None)
            .map_err(|e| e.message())?;
        let mut data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        let made = SetupDiCreateDeviceInfoW(
            set,
            w!("Display"),
            &GUID_DEVCLASS_DISPLAY,
            PCWSTR::null(),
            None,
            DICD_GENERATE_ID,
            Some(&mut data),
        )
        .and_then(|()| {
            SetupDiSetDeviceRegistryPropertyW(set, &mut data, SPDRP_HARDWAREID, Some(&bytes))
        })
        .and_then(|()| SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set, Some(&data)));
        let _ = SetupDiDestroyDeviceInfoList(set);
        made.map_err(|e| format!("making its device: {}", e.message()))
    }
}

fn remove_sudovda(installed: &Installed) -> Result<(), String> {
    if installed.sudovda_device {
        let mut failed = None;
        each_device(SUDOVDA_HWID, |set, data| {
            // SAFETY: `set` and `data` are the enumeration's, alive for the
            // call.
            if let Err(e) = unsafe { DiUninstallDevice(HWND::default(), set, data, 0, None) } {
                failed = Some(e.message());
            }
            false
        })?;
        if let Some(e) = failed {
            return Err(format!("removing its device: {e}"));
        }
    }
    if let Some(package) = &installed.sudovda_package {
        // SAFETY: the name is NUL-terminated and lives for the call.
        let ok = unsafe {
            SetupUninstallOEMInfW(&HSTRING::from(package.as_str()), SUOI_FORCEDELETE, None)
        };
        if !ok.as_bool() {
            return Err(format!(
                "removing its driver package {package}: {}",
                windows::core::Error::from_win32().message()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_string_reads_as_its_strings() {
        let bytes: Vec<u8> = "Root\\A\0B\0\0junk"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert_eq!(multi_sz(&bytes), ["Root\\A", "B"]);
    }

    #[test]
    fn the_embedded_sudovda_is_the_vendored_one() {
        // vendor/sudovda/README.md's hashes.
        assert!(matches_sha256(
            SUDOVDA_DLL,
            "47ee263cb5de9382c6630a2d7f3dafec4a49419f953beec869ca5dd0c460ff63"
        ));
        assert!(matches_sha256(
            SUDOVDA_CERT,
            "6accdcd519f6179d967db4eaa20ecf25a732ba30e87f4cffebc768b2c13c9007"
        ));
    }

    #[test]
    fn what_was_installed_is_read_back() {
        let dir = std::env::temp_dir().join(format!("helio-drivers-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("drivers")).unwrap();
        let installed = Installed {
            sudovda_device: true,
            sudovda_package: Some("oem42.inf".into()),
            sudovda_stores: vec!["Root".into()],
            vigembus: false,
        };
        installed.write(&dir);
        let read = Installed::read(&dir);
        assert!(read.sudovda_device && !read.vigembus);
        assert_eq!(read.sudovda_package.as_deref(), Some("oem42.inf"));
        assert_eq!(read.sudovda_stores, ["Root"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
