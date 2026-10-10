//! Real WebDAV server fixture for contract tests. Spawns the macOS bundled
//! Apache httpd with mod_dav (the reference WebDAV implementation: conditional
//! PUT via If-None-Match / RFC 4918 If headers, LOCK/423, Basic auth) on an
//! uncommon loopback port with a per-test data directory. Each instance is
//! stopped by its own Drop; no global process is touched.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const USERNAME: &str = "wardrobe-it";
pub const PASSWORD: &str = "contract fixture password";

pub struct WebDavServer {
    pub base_url: String,
    httpd: PathBuf,
    config: PathBuf,
    root: PathBuf,
    _child: Option<Child>,
}

impl WebDavServer {
    /// Spawns a real Apache mod_dav server. `#[ignore]`d tests only: the binary
    /// ships with macOS; absence fails loudly instead of silently skipping.
    pub fn spawn() -> Self {
        let httpd: PathBuf = std::env::var("WARDROBE_HTTPD_BIN")
            .unwrap_or_else(|_| "/usr/sbin/httpd".to_string())
            .into();
        if !httpd.is_file() {
            panic!(
                "real WebDAV contract tests require Apache httpd ({})",
                httpd.display()
            );
        }
        let root = tempfile::tempdir_in(external_volume())
            .expect("temp root")
            .keep();
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::create_dir_all(root.join("locks")).unwrap();
        std::fs::create_dir_all(root.join("logs")).unwrap();
        let htpasswd = root.join("htpasswd");
        let output = Command::new("/usr/sbin/htpasswd")
            .args(["-b", "-c", "-m"])
            .arg(&htpasswd)
            .arg(USERNAME)
            .arg(PASSWORD)
            .output()
            .expect("htpasswd");
        assert!(output.status.success(), "htpasswd failed: {:?}", output);
        let modules = "/usr/libexec/apache2";
        let config = root.join("httpd.conf");
        let port = port();
        let access_log_format = "%h %u %{%s}t \\\"%r\\\" %>s %B";
        std::fs::write(
            &config,
            format!(
                "ServerName localhost\n\
                 Listen 127.0.0.1:{port}\n\
                 PidFile {root}/httpd.pid\n\
                 ErrorLog {root}/logs/error.log\n\
                 CustomLog {root}/logs/access.log \"{access_log_format}\"\n\
                 LoadModule unixd_module {modules}/mod_unixd.so\n\
                 LoadModule mpm_prefork_module {modules}/mod_mpm_prefork.so\n\
                 LoadModule authn_core_module {modules}/mod_authn_core.so\n\
                 LoadModule authz_core_module {modules}/mod_authz_core.so\n\
                 LoadModule auth_basic_module {modules}/mod_auth_basic.so\n\
                 LoadModule authn_file_module {modules}/mod_authn_file.so\n\
                 LoadModule authz_user_module {modules}/mod_authz_user.so\n\
                 LoadModule log_config_module {modules}/mod_log_config.so\n\
                 LoadModule dav_module {modules}/mod_dav.so\n\
                 LoadModule dav_fs_module {modules}/mod_dav_fs.so\n\
                 LoadModule dav_lock_module {modules}/mod_dav_lock.so\n\
                 DavLockDB {root}/locks/DavLock\n\
                 User {user}\n\
                 Group staff\n\
                 DocumentRoot \"{root}/data\"\n\
                 <Directory \"{root}/data\">\n\
                 \x20 Options Indexes\n\
                 \x20 AllowOverride None\n\
                 \x20 Dav On\n\
                 \x20 AuthType Basic\n\
                 \x20 AuthName wardrobe-contract\n\
                 \x20 AuthUserFile {htpasswd}\n\
                 \x20 Require valid-user\n\
                 </Directory>\n",
                port = port,
                root = root.display(),
                modules = modules,
                user = whoami(),
                htpasswd = htpasswd.display(),
            ),
        )
        .unwrap();
        let started = Command::new(&httpd)
            .args(["-f"])
            .arg(&config)
            .arg("-k")
            .arg("start")
            .stderr(Stdio::piped())
            .output()
            .expect("spawn httpd");
        assert!(
            started.status.success(),
            "httpd -k start failed: {} {}",
            String::from_utf8_lossy(&started.stderr),
            std::fs::read_to_string(root.join("logs/error.log")).unwrap_or_default()
        );
        let base_url = format!("http://127.0.0.1:{port}");
        let ready = Instant::now();
        loop {
            if ready_again(&base_url) {
                break;
            }
            assert!(
                ready.elapsed() < Duration::from_secs(15),
                "fixture WebDAV server never became ready"
            );
            std::thread::sleep(Duration::from_millis(120));
        }
        Self {
            base_url,
            httpd,
            config,
            root,
            _child: None,
        }
    }

    /// Server-side data directory; used by fault-injection fixtures that must
    /// corrupt or inspect what a real server persisted.
    pub fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }

    fn stop(&self) {
        let _ = Command::new(&self.httpd)
            .args(["-f"])
            .arg(&self.config)
            .arg("-k")
            .arg("graceful-stop")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl Drop for WebDavServer {
    fn drop(&mut self) {
        self.stop();
        let deadline = Instant::now();
        while deadline.elapsed() < Duration::from_secs(5) {
            if !self.root.join("httpd.pid").exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if std::env::var("WARDROBE_WEBDAV_KEEP").as_deref() == Ok("1") {
            eprintln!(
                "WARDROBE_WEBDAV_KEEP=1: keeping fixture dir {}",
                self.root.display()
            );
            return;
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn whoami() -> String {
    std::env::var("USER").unwrap_or_else(|_| "nobody".to_string())
}

fn external_volume() -> PathBuf {
    let volume = "/Users/links/.codex/external-workspace-volume/codex/build-temp";
    if Path::new(volume).is_dir() {
        return volume.into();
    }
    std::env::temp_dir()
}

fn ready_again(base_url: &str) -> bool {
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-u",
            &format!("{USERNAME}:{PASSWORD}"),
            "-X",
            "PROPFIND",
            "-H",
            "Depth: 0",
            &format!("{base_url}/"),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("curl");
    let mut out = String::new();
    let read = std::io::Read::read_to_string(child.stdout.as_mut().expect("stdout"), &mut out);
    let _ = child.wait();
    read.is_ok() && out.trim() == "207"
}

fn port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind :0")
        .local_addr()
        .expect("local addr")
        .port()
}
