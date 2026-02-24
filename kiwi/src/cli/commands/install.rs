use crate::cli::error::{CliError, CliResult};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

pub fn run() -> CliResult<()> {
    #[derive(Debug, PartialEq)]
    enum InstallMode {
        User,
        Root,
    }

    // 1. Setup Configuration
    let is_root = get_uid().map_err(|e| CliError::new(format!("failed to resolve uid: {e}")))? == "0";
    let mode = if is_root {
        InstallMode::Root
    } else {
        InstallMode::User
    };
    let app_name = "Kiwi";
    let bundle_id = "com.example.kiwi";

    println!("🚀 Installing in {:?} mode...", mode);

    // 2. Define Paths
    let home = env::var("HOME").map_err(|e| CliError::new(format!("HOME is not set: {e}")))?;
    let (base_app_dir, launch_dir, log_dir, domain) = match mode {
        InstallMode::User => (
            PathBuf::from(&home).join("Applications"),
            PathBuf::from(&home).join("Library/LaunchAgents"),
            PathBuf::from(&home).join("Library/Logs/Kiwi"),
            format!(
                "gui/{}",
                get_uid().map_err(|e| CliError::new(format!("failed to resolve uid: {e}")))?
            ),
        ),
        InstallMode::Root => (
            PathBuf::from("/Applications"),
            PathBuf::from("/Library/LaunchDaemons"),
            PathBuf::from("/Library/Logs/Kiwi"),
            "system".to_string(),
        ),
    };

    let app_path = base_app_dir.join(format!("{}.app", app_name));
    let exec_dest = app_path.join("Contents/MacOS/kiwi");
    let plist_path = launch_dir.join(format!("{}.plist", bundle_id));

    // Find source binary
    let cargo_bin = PathBuf::from(&home).join(".cargo/bin/kiwi");
    if !cargo_bin.exists() {
        return Err(CliError::new(
            "Source kiwi binary not found in ~/.cargo/bin. Run 'cargo install --path kiwi --force' first.",
        ));
    }

    // 3. Pre-install Cleanup
    println!("🧹 Cleaning up existing service and bundle...");
    let _ = Command::new("launchctl")
        .args(["bootout", &domain, plist_path.to_str().unwrap()])
        .output();
    if app_path.exists() {
        fs::remove_dir_all(&app_path)
            .map_err(|e| CliError::new(format!("failed to remove old app bundle: {e}")))?;
    }
    if log_dir.exists() {
        fs::remove_dir_all(&log_dir)
            .map_err(|e| CliError::new(format!("failed to clear old logs: {e}")))?;
    }
    fs::create_dir_all(app_path.join("Contents/MacOS"))
        .map_err(|e| CliError::new(format!("failed to create app directory: {e}")))?;
    fs::create_dir_all(&log_dir)
        .map_err(|e| CliError::new(format!("failed to create log directory: {e}")))?;

    // 4. COPY Binary (Crucial: replaces the symlink)
    println!("📦 Copying binary into bundle...");
    fs::copy(&cargo_bin, &exec_dest)
        .map_err(|e| CliError::new(format!("failed to copy binary: {e}")))?;

    // 5. Fix Permissions & Strip Quarantine
    println!("🔑 Setting permissions and clearing quarantine...");
    let mut perms = fs::metadata(&exec_dest)
        .map_err(|e| CliError::new(format!("failed to read binary metadata: {e}")))?
        .permissions();
    perms.set_mode(0o755); // Read/Execute
    fs::set_permissions(&exec_dest, perms)
        .map_err(|e| CliError::new(format!("failed to set binary permissions: {e}")))?;

    // Remove the '@' attributes (quarantine)
    let _ = Command::new("xattr").arg("-cr").arg(&app_path).status();

    // 6. Write Plists
    let stdout_log = log_dir.join("stdout.log");
    let stderr_log = log_dir.join("stderr.log");

    fs::write(
        app_path.join("Contents/Info.plist"),
        generate_info_plist(app_name, bundle_id),
    )
    .map_err(|e| CliError::new(format!("failed to write Info.plist: {e}")))?;
    fs::write(
        &plist_path,
        generate_launch_plist(bundle_id, &exec_dest, &stdout_log, &stderr_log),
    )
    .map_err(|e| CliError::new(format!("failed to write launch plist: {e}")))?;

    // 7. Bootstrap
    println!("⚡ Bootstrapping service...");
    Command::new("launchctl")
        .args(["bootstrap", &domain, plist_path.to_str().unwrap()])
        .status()
        .map_err(|e| CliError::new(format!("failed to bootstrap service: {e}")))?;

    // 8. Reactive Monitoring Loop
    let mut attempts = 0;
    let max_attempts = 2;

    while attempts < max_attempts {
        println!("⏳ Verifying service health (Attempt {})...", attempts + 1);
        thread::sleep(Duration::from_secs(2)); // Give it a moment to start/fail

        if is_service_running(bundle_id, &domain)
            .map_err(|e| CliError::new(format!("failed to inspect service state: {e}")))?
        {
            println!("✅ Kiwi is running with valid permissions!");
            return Ok(());
        }

        println!("⚠️  Kiwi failed to stay running. Likely missing Accessibility permissions.");

        // Open settings and WAIT for the user to close the window
        open_settings_and_wait();

        thread::sleep(Duration::from_secs(2));

        // Tell launchctl to try again immediately
        println!("🔄 Retrying service...");
        let _ = Command::new("launchctl")
            .args(["kickstart", "-p", &format!("{}/{}", domain, bundle_id)])
            .status();

        attempts += 1;
    }

    eprintln!(
        "❌ Failed to start Kiwi after {} attempts. Please check permissions manually.",
        max_attempts
    );
    Ok(())
}

fn is_service_running(label: &str, domain: &str) -> Result<bool, std::io::Error> {
    let output = Command::new("launchctl")
        .args(["print", &format!("{}/{}", domain, label)])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // If the state is 'active' and there is a 'pid', it's running.
    // If it says 'last exit code = [some error]', it's failing.
    Ok(stdout.contains("state = active") && stdout.contains("pid ="))
}

fn open_settings_and_wait() {
    println!(
        "📢 Opening Accessibility settings. Please enable 'Kiwi' and then CLOSE the Settings window to continue."
    );

    // Using '-W' with 'open' makes the command block until the app is closed
    let _ = Command::new("open")
        .args([
            "-W",
            "-a",
            "System Settings",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        ])
        .status();

    println!("🔍 Settings window closed. Resuming...");
}
fn get_uid() -> Result<String, std::io::Error> {
    let output = Command::new("id").arg("-u").output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn generate_info_plist(name: &str, id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>kiwi</string>
    <key>CFBundleIdentifier</key><string>{}</string>
    <key>CFBundleName</key><string>{}</string>
    <key>LSUIElement</key><true/>
</dict>
</plist>"#,
        id, name
    )
}

fn generate_launch_plist(id: &str, exec: &Path, out: &Path, err: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{}</string>
    <key>ProgramArguments</key>
    <array><string>{}</string></array>
    <key>RunAtLoad</key><true/>
    <key>StandardOutPath</key><string>{}</string>
    <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>"#,
        id,
        exec.to_str().unwrap(),
        out.to_str().unwrap(),
        err.to_str().unwrap()
    )
}
