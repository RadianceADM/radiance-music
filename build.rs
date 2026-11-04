//! Windows: the program's own details and icon, inside radiance-music.exe (owner,
//! 2026-11-04: Microsoft Defender's machine-learning guess flagged 0.7.16 as
//! "Trojan:Win32/Bearfoos.A!ml"; an unsigned file with no version information and no
//! icon looks least like a normal app). Shown in the file's Properties → Details and by
//! Explorer. Built with windres (MSYS2's on ribs03, mingw's when cross-building). Nothing
//! is done for Linux.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=packaging/windows/radiance-music.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let version = env::var("CARGO_PKG_VERSION").expect("version");
    let n: Vec<u16> = version.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).chain([0, 0, 0]).take(3).collect();
    let icon = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("packaging/windows/radiance-music.ico");
    // (forward slashes: windres, like GStreamer's parser, reads a backslash as an escape)
    let icon = icon.display().to_string().replace('\\', "/");
    let rc = format!(
        r#"#include <winver.h>
1 ICON "{icon}"
VS_VERSION_INFO VERSIONINFO
FILEVERSION {a},{b},{c},0
PRODUCTVERSION {a},{b},{c},0
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Radiance Innovation Centers"
      VALUE "FileDescription", "Radiance Music"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "radiance-music"
      VALUE "LegalCopyright", "Copyright (C) 2026 Radiance Innovation Centers. Free software: GPL-3.0-or-later."
      VALUE "OriginalFilename", "radiance-music.exe"
      VALUE "ProductName", "Radiance Music"
      VALUE "ProductVersion", "{version}"
      VALUE "Comments", "Open source: https://github.com/RadianceADM/radiance-music"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        a = n[0],
        b = n[1],
        c = n[2],
    );
    let rc_file = out.join("radiance-music.rc");
    fs::write(&rc_file, rc).expect("writing the resource script");
    let obj = out.join("radiance-music-res.o");
    // the cross-compiler's windres on Linux, MSYS2's own on Windows
    let windres = env::var("WINDRES").unwrap_or_else(|_| if cfg!(windows) { "windres".into() } else { "x86_64-w64-mingw32-windres".into() });
    let done = Command::new(&windres).args(["-O", "coff", "-i"]).arg(&rc_file).arg("-o").arg(&obj).status();
    match done {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", obj.display()),
        other => panic!("{windres} couldn't make the program's details and icon: {other:?}"),
    }
}
