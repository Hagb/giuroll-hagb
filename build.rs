use std::env;

use winres::{VersionInfo, WindowsResource};

extern crate winres;

static VERSION_REMARK: Option<&str> = Some("(fork by Hagb)");
static DLL_REVISION: u16 = 3;
fn main() {
    let mut res = WindowsResource::new();
    if cfg!(unix) {
        // from https://github.com/mxre/winres/blob/1807bec3552cd2f0d0544420584d6d78be5e3636/example/build.rs#L10
        res.set_toolkit_path("/home/hagb/my_msvc/");
        // ar tool for mingw in toolkit path
        res.set_ar_path("/usr/i686-w64-mingw32/bin/ar");
        // windres tool
        res.set_windres_path("/usr/bin/i686-w64-mingw32-windres");
    }

    let mut version = 0_u64;
    version |= env::var("CARGO_PKG_VERSION_MAJOR")
        .unwrap()
        .parse::<u64>()
        .unwrap()
        << 48;
    version |= env::var("CARGO_PKG_VERSION_MINOR")
        .unwrap()
        .parse::<u64>()
        .unwrap()
        << 32;
    version |= env::var("CARGO_PKG_VERSION_PATCH")
        .unwrap()
        .parse::<u64>()
        .unwrap()
        << 16;
    version |= DLL_REVISION as u64;
    res.set_version_info(VersionInfo::FILEVERSION, version);
    res.set_version_info(VersionInfo::PRODUCTVERSION, version);

    res.set(
        "LegalCopyright",
        format!(
            "Copyright (c) {}",
            env::var("CARGO_PKG_AUTHORS")
                .unwrap()
                .split(":")
                .collect::<Vec<_>>()
                .join(", ")
        )
        .as_str(),
    );
    res.set("ProductName", env::var("CARGO_PKG_NAME").unwrap().as_str());
    res.set(
        "FileDescription",
        env::var("CARGO_PKG_DESCRIPTION").unwrap().as_str(),
    );
    res.set(
        "ProductVersion",
        format!(
            "{}{}{}",
            env::var("CARGO_PKG_VERSION").unwrap(),
            match VERSION_REMARK {
                Some(remark) => " ".to_string() + &remark,
                None => "".to_string(),
            },
            env::var("SOURCE_URL")
                .and_then(|x| Ok(format!(" ({})", x).to_string()))
                .unwrap_or("".to_string())
        )
        .as_str(),
    );

    println!("cargo:rustc-env=DLL_REVISION={}", DLL_REVISION);
    if let Some(remark) = VERSION_REMARK {
        println!("cargo:rustc-env=VERSION_REMARK={}", remark);
    }
    println!("cargo:rustc-env=DLL_VERSION={}", version);

    if let Err(e) = res.compile() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
