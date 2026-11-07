//! Letting the phone in for Share Songs with a Phone (moved out of the Linux app's src/ui/sharing.rs, stage 4b).
//! Linux: Ubuntu's firewall (ufw). Windows: each app's own rule, tied to its own program (stage 4b ruling: the
//! beta's rule isn't the Linux-built Windows app's, so one let in doesn't look like the other's).

use crate::share::PORTS;

/// Today's Windows app's rule, and the beta's.
pub const APP_RULE: &str = "Radiance Music (songs to and from a phone)";
pub const BETA_RULE: &str = "Radiance Music (beta) (songs to and from a phone)";

/// This app's rule's name.
pub fn rule() -> &'static str {
    if crate::update::is_beta() {
        BETA_RULE
    } else {
        APP_RULE
    }
}

/// The firewall may keep the phone out: Ubuntu's is on; Windows' is on and this app's rule isn't there yet
/// (Windows asks the first time the app listens; a "no" there blocks the phone, and nothing would say). Worker thread.
#[cfg(unix)]
pub fn blocks() -> bool {
    std::fs::read_to_string("/etc/ufw/ufw.conf").is_ok_and(|c| c.lines().any(|l| l.trim() == "ENABLED=yes"))
}

#[cfg(windows)]
pub fn blocks() -> bool {
    let mut cmd = std::process::Command::new("netsh");
    cmd.args(["advfirewall", "firewall", "show", "rule", &format!("name={}", rule())]);
    crate::setup::quiet_window(&mut cmd, false);
    !cmd.output().is_ok_and(|o| o.status.success())
}

/// Lets the phone in: the sharing ports, from home networks only (asks for the password; worker thread).
#[cfg(unix)]
pub fn let_in() -> bool {
    let ports = format!("{}:{}", PORTS.start(), PORTS.end());
    let rules: Vec<String> = ["192.168.0.0/16", "10.0.0.0/8", "172.16.0.0/12"]
        .iter()
        .map(|net| format!("ufw allow proto tcp from {net} to any port {ports} comment 'Radiance Music: songs from and to a phone'"))
        .collect();
    std::process::Command::new("pkexec").args(["sh", "-c", &rules.join(" && ")]).status().is_ok_and(|s| s.success())
}

/// Windows: the app's own rules go first (Windows makes one from its question, a block if it was told no, and a
/// block wins over an allow), then the sharing ports from the home network, on any network type (Windows calls a
/// new Wi-Fi "public"). One Windows prompt to allow it; worker thread.
#[cfg(windows)]
pub fn let_in() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let exe = exe.display().to_string();
    let rules = format!(
        "netsh advfirewall firewall delete rule name=all dir=in program=\"{exe}\" & netsh advfirewall firewall add rule name=\"{}\" dir=in action=allow program=\"{exe}\" protocol=TCP localport={}-{} remoteip=localsubnet profile=any",
        rule(),
        PORTS.start(),
        PORTS.end()
    );
    // PowerShell asks Windows to run it as administrator (the prompt), and says how it went
    let script = format!("$p = Start-Process -Verb RunAs -Wait -PassThru -WindowStyle Hidden cmd -ArgumentList '/c {}'; exit $p.ExitCode", rules.replace('\'', "''"));
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    crate::setup::quiet_window(&mut cmd, false);
    cmd.status().is_ok_and(|s| s.success()) && !blocks()
}

#[cfg(test)]
mod tests {
    #[test]
    fn each_app_has_its_own_rule() {
        assert_ne!(super::APP_RULE, super::BETA_RULE);
        assert_eq!(super::rule(), super::APP_RULE, "not the beta: today's app's rule");
    }
}
