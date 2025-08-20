use std::{
  env, fs,
  path::{Path, PathBuf},
  process::Command,
};

struct ProbedLib {
  version: String,
  include_paths: Vec<PathBuf>,
  // Extra clang args to pass to bindgen (e.g. target/sysroot/defines)
  clang_args: Vec<String>,
}

/// Finds libxml2 and optionally return a list of header
/// files from which the bindings can be generated.
fn find_libxml2() -> Option<ProbedLib> {
  #![allow(unreachable_code)] // for platform-dependent dead code

  if let Ok(ref s) = std::env::var("LIBXML2") {
    // println!("{:?}", std::env::vars());
    // panic!("set libxml2.");
    let p = std::path::Path::new(s);
    let fname = std::path::Path::new(
      p.file_name()
        .unwrap_or_else(|| panic!("no file name in LIBXML2 env ({s})")),
    );
    assert!(
      p.is_file(),
      "{}",
      &format!("not a file in LIBXML2 env ({s})")
    );
    println!(
      "cargo:rustc-link-lib={}",
      fname
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .strip_prefix("lib")
        .unwrap()
    );
    println!(
      "cargo:rustc-link-search={}",
      p.parent()
        .expect("no library path in LIBXML2 env")
        .to_string_lossy()
    );
    None
  } else {
    let target = env::var("TARGET").unwrap_or_default();

    // Android: try NDK + CMake lazy build of libxml2
    if target.contains("android") {
      println!("cargo:rerun-if-env-changed=ANDROID_NDK_ROOT");
      println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");
      let ndk = env::var("ANDROID_NDK_ROOT").or_else(|_| env::var("ANDROID_NDK_HOME"));
      let ndk_root = match ndk {
        Ok(p) => PathBuf::from(p),
        Err(_) => panic!("Android target detected, but ANDROID_NDK_ROOT/ANDROID_NDK_HOME not set"),
      };
      // Early check for cmake presence to provide clearer diagnostics
      if which::which("cmake").is_err() {
        panic!(
          "CMake not found. Please install CMake and ensure it's on PATH (Windows: install from cmake.org and reopen the terminal; Linux: apt/dnf/pacman install). Optional but recommended: install Ninja."
        );
      }
      let api_level = env::var("ANDROID_PLATFORM")
        .ok()
        .and_then(|v| v.trim_start_matches("android-").parse::<u32>().ok())
        .unwrap_or(21);
      let (android_abi, clang_target) = map_android_abi_and_target(&target, api_level)
        .unwrap_or_else(|| panic!("Unsupported Android target triple: {target}"));
      let host_tag = ndk_host_tag();
      // Compute NDK sysroot for bindgen
      let sysroot = ndk_root
        .join("toolchains/llvm/prebuilt")
        .join(&host_tag)
        .join("sysroot");

      // auto set CC/CXX environment variables
      let bin_dir = ndk_root
        .join("toolchains/llvm/prebuilt")
        .join(&host_tag)
        .join("bin");
      let cc_path = bin_dir.join(format!("{}-clang.cmd", clang_target));
      let cxx_path = bin_dir.join(format!("{}-clang++.cmd", clang_target));
      if cc_path.exists() && cxx_path.exists() {
        unsafe {
          env::set_var("CC", &cc_path);
          env::set_var("CXX", &cxx_path);
        }
      }

      let (dst, include_dir) =
        build_libxml2_android(&ndk_root, &host_tag, android_abi, &clang_target, api_level);

      // Link to the freshly built static libxml2
      println!(
        "cargo:rustc-link-search=native={}",
        dst.join("lib").display()
      );
      println!("cargo:rustc-link-lib=static=xml2");

      let mut clang_args = vec![
        format!("--target={}", clang_target),
        String::from("-D__ANDROID__"),
      ];
      // libxml2 headers
      clang_args.push(format!("-I{}", include_dir.display()));
      if sysroot.exists() {
        // Prefer combined form for reliability on Windows
        clang_args.push(format!("--sysroot={}", sysroot.display()));
        // Also add common include roots under sysroot
        let sys_inc = sysroot.join("usr/include");
        if sys_inc.exists() {
          clang_args.push(format!("-I{}", sys_inc.display()));
        }
        // Arch-specific headers
        let arch = arch_triple_for_sysroot(&clang_target);
        let arch_inc = sys_inc.join(&arch);
        if arch_inc.exists() {
          clang_args.push(format!("-I{}", arch_inc.display()));
        }
      }

      return Some(ProbedLib {
        include_paths: vec![include_dir],
        version: String::from("2.13.0"),
        clang_args,
      });
    }

    // iOS (only supported on macOS hosts): use SDK-provided libxml2
    if target.contains("apple-ios") {
      // Only allow on macOS hosts
      let host = env::var("HOST").unwrap_or_default();
      if !host.contains("apple-darwin") {
        panic!("iOS builds are only supported on macOS hosts");
      }
      let is_sim = target.contains("-sim");
      let sdk = if is_sim {
        "iphonesimulator"
      } else {
        "iphoneos"
      };
      let sdk_path = xcrun_sdk_path(sdk).expect("Failed to resolve iOS SDK via xcrun");
      let include_dir = sdk_path.join("usr/include/libxml2");
      let lib_dir = sdk_path.join("usr/lib");

      // Link against the SDK's libxml2.tbd
      println!("cargo:rustc-link-search=native={}", lib_dir.display());
      println!("cargo:rustc-link-lib=xml2");

      let mut clang_target = target.replace("-apple-ios-sim", "-apple-ios-simulator");
      if target == "aarch64-apple-ios" {
        clang_target = "arm64-apple-ios".to_string();
      }
      let clang_args = vec![
        format!("--target={}", clang_target),
        format!("-isysroot"),
        sdk_path.display().to_string(),
        format!("-I{}", include_dir.display()),
      ];

      return Some(ProbedLib {
        include_paths: vec![include_dir],
        version: String::from("2.13.0"),
        clang_args,
      });
    }

    #[cfg(any(
      target_family = "unix",
      target_os = "macos",
      all(target_family = "windows", target_env = "gnu")
    ))]
    {
      let lib = pkg_config::Config::new()
        .probe("libxml-2.0")
        .expect("Couldn't find libxml2 via pkg-config");
      return Some(ProbedLib {
        include_paths: lib.include_paths,
        version: lib.version,
        clang_args: vec![],
      });
    }

    #[cfg(all(target_family = "windows", target_env = "msvc"))]
    {
      if let Some(meta) = vcpkg_dep::vcpkg_find_libxml2() {
        return Some(meta);
      } else {
        eprintln!("vcpkg did not succeed in finding libxml2.");
      }
    }

    panic!("Could not find libxml2.")
  }
}

// Resolve iOS SDK path via xcrun
fn xcrun_sdk_path(sdk: &str) -> Option<PathBuf> {
  let out = Command::new("xcrun")
    .args(["--sdk", sdk, "--show-sdk-path"])
    .output()
    .ok()?;
  if !out.status.success() {
    return None;
  }
  let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
  if s.is_empty() {
    None
  } else {
    Some(PathBuf::from(s))
  }
}

// Map Android Rust target triple to (ABI, clang target with API level)
fn map_android_abi_and_target(target: &str, api: u32) -> Option<(&'static str, String)> {
  let t = if let Some(stripped) = target.strip_suffix("eabi") {
    stripped
  } else {
    target
  };
  match t {
    "aarch64-linux-android" => Some(("arm64-v8a", format!("aarch64-linux-android{}", api))),
    "armv7-linux-android" | "armv7-linux-androideabi" => {
      Some(("armeabi-v7a", format!("armv7a-linux-androideabi{}", api)))
    }
    "i686-linux-android" => Some(("x86", format!("i686-linux-android{}", api))),
    "x86_64-linux-android" => Some(("x86_64", format!("x86_64-linux-android{}", api))),
    _ => None,
  }
}

// Clone and build libxml2 for Android using CMake + NDK toolchain
fn build_libxml2_android(
  ndk_root: &Path,
  host_tag: &str,
  abi: &str,
  clang_target: &str,
  api: u32,
) -> (PathBuf, PathBuf) {
  // Prepare paths
  let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
  let src_dir = out_dir.join("libxml2-src");
  let repo_url =
    env::var("LIBXML2_GIT").unwrap_or_else(|_| "https://github.com/GNOME/libxml2.git".to_string());
  if !src_dir.exists() {
    // Shallow clone for speed
    let status = Command::new("git")
      .args([
        "clone",
        "--depth",
        "1",
        "--branch",
        "v2.13.5",
        &repo_url,
        src_dir.to_string_lossy().as_ref(),
      ])
      .status()
      .expect("failed to spawn git");
    if !status.success() {
      panic!("git clone of libxml2 failed");
    }
  }

  // 自动清理 CMakeCache.txt 和 CMakeFiles/，防止 cache 污染
  let build_dir = out_dir.join("build");
  let cache_file = build_dir.join("CMakeCache.txt");
  if cache_file.exists() {
    let _ = std::fs::remove_file(&cache_file);
  }
  let cmakefiles_dir = build_dir.join("CMakeFiles");
  if cmakefiles_dir.exists() {
    let _ = std::fs::remove_dir_all(&cmakefiles_dir);
  }

  // Configure build with cmake
  let mut cfg = cmake::Config::new(&src_dir);
  cfg.profile("Release");
  cfg.define(
    "CMAKE_TOOLCHAIN_FILE",
    ndk_root.join("build/cmake/android.toolchain.cmake"),
  );
  cfg.define("ANDROID_ABI", abi);
  cfg.define(
    "ANDROID_PLATFORM",
    env::var("ANDROID_PLATFORM").unwrap_or_else(|_| "21".to_string()),
  );
  cfg.define("BUILD_SHARED_LIBS", "OFF");
  // Feature trims to minimize deps
  cfg.define("LIBXML2_WITH_PYTHON", "OFF");
  cfg.define("LIBXML2_WITH_LZMA", "OFF");
  cfg.define("LIBXML2_WITH_ZLIB", "OFF");
  cfg.define("LIBXML2_WITH_ICONV", "OFF");
  cfg.define("LIBXML2_WITH_TESTS", "OFF");
  cfg.define("LIBXML2_WITH_PROGRAMS", "OFF");

  // Prefer Ninja and require it on Windows to avoid MSBuild generator issues
  match which::which("ninja") {
    Ok(ninja_path) => {
      cfg.generator("Ninja");
      cfg.define("CMAKE_MAKE_PROGRAM", ninja_path);
    }
    Err(_) => {
      #[cfg(target_os = "windows")]
      {
        panic!(
          "Ninja not found. On Windows, please install Ninja (scoop install ninja) to build Android with CMake+NDK."
        );
      }
      // On non-Windows we let CMake pick a default, though Ninja is still recommended
    }
  }

  // Point to explicit compilers from NDK to avoid tool name mismatches
  let prebuilt = ndk_root.join("toolchains/llvm/prebuilt").join(host_tag);
  let bin = prebuilt.join("bin");
  let sysroot = prebuilt.join("sysroot");
  // Prefer API-suffixed compilers; fallback to plain
  let cc_api = bin.join(format!("{}-clang", clang_target));
  let cxx_api = bin.join(format!("{}-clang++", clang_target));
  let plain = clang_target.trim_end_matches(|c: char| c.is_ascii_digit());
  let cc_plain = bin.join(format!("{}-clang", plain));
  let cxx_plain = bin.join(format!("{}-clang++", plain));

  #[cfg(target_os = "windows")]
  fn with_cmd(p: PathBuf) -> PathBuf {
    let mut x = p;
    if x.extension().is_none() {
      x.set_extension("cmd");
    }
    x
  }
  #[cfg(not(target_os = "windows"))]
  fn with_cmd(p: PathBuf) -> PathBuf {
    p
  }

  let cc = if cc_api.exists() {
    with_cmd(cc_api)
  } else {
    with_cmd(cc_plain)
  };
  let cxx = if cxx_api.exists() {
    with_cmd(cxx_api)
  } else {
    with_cmd(cxx_plain)
  };

  cfg.define("CMAKE_C_COMPILER", &cc);
  cfg.define("CMAKE_CXX_COMPILER", &cxx);
  cfg.define("CMAKE_SYSROOT", &sysroot);
  // Force Android system to prevent Windows detection
  cfg.define("CMAKE_SYSTEM_NAME", "Android");
  cfg.define("CMAKE_SYSTEM_VERSION", api.to_string());
  cfg.define("CMAKE_ANDROID_ARCH_ABI", abi);
  // Remove conflicting target flags - let NDK toolchain handle this
  cfg.define("CMAKE_C_FLAGS", "");
  cfg.define("CMAKE_CXX_FLAGS", "");
  cfg.define("CMAKE_ASM_FLAGS", "");

  let dst = cfg.build();
  let include_dir = dst.join("include").join("libxml2");
  if !include_dir.exists() {
    panic!(
      "libxml2 include directory not found at {}",
      include_dir.display()
    );
  }
  (dst, include_dir)
}

fn ndk_host_tag() -> String {
  let host = env::var("HOST").unwrap_or_default();
  if host.contains("windows") {
    return "windows-x86_64".to_string();
  }
  if host.contains("apple-darwin") {
    if host.starts_with("aarch64-") {
      return "darwin-arm64".to_string();
    }
    return "darwin-x86_64".to_string();
  }
  // linux default
  "linux-x86_64".to_string()
}

// Map clang target to sysroot arch include subdir
fn arch_triple_for_sysroot(clang_target: &str) -> String {
  if clang_target.starts_with("aarch64-") {
    return "aarch64-linux-android".to_string();
  }
  if clang_target.starts_with("armv7") || clang_target.contains("androideabi") {
    return "arm-linux-androideabi".to_string();
  }
  if clang_target.starts_with("i686-") {
    return "i686-linux-android".to_string();
  }
  if clang_target.starts_with("x86_64-") {
    return "x86_64-linux-android".to_string();
  }
  // fallback: strip trailing digits (API) if any
  clang_target
    .trim_end_matches(|c: char| c.is_ascii_digit())
    .trim_end_matches('-')
    .to_string()
}

fn generate_bindings(header_dirs: Vec<PathBuf>, extra_clang_args: &[String], output_path: &Path) {
  let bindings = bindgen::Builder::default()
    .header("src/wrapper.h")
    .opaque_type("max_align_t")
    // invalidate build as soon as the wrapper changes
    .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
    .layout_tests(true)
    .clang_args(&[
      "-DPKG-CONFIG",
      "-DLIBXML_C14N_ENABLED",
      "-DLIBXML_OUTPUT_ENABLED",
    ])
    .clang_args(header_dirs.iter().map(|dir| format!("-I{}", dir.display())))
    .clang_args(extra_clang_args);
  bindings
    .generate()
    .expect("failed to generate bindings with bindgen")
    .write_to_file(output_path)
    .expect("Failed to write bindings.rs");
}

fn main() {
  let bindings_path = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("bindings.rs");
  // declare availability of config variable (without setting it)
  println!("cargo::rustc-check-cfg=cfg(libxml_older_than_2_12)");

  if let Some(probed_lib) = find_libxml2() {
    // if we could find header files, generate fresh bindings from them
    generate_bindings(
      probed_lib.include_paths,
      &probed_lib.clang_args,
      &bindings_path,
    );
    // and expose the libxml2 version to the code
    let version_parts: Vec<i32> = probed_lib
      .version
      .split('.')
      .map(|part| part.parse::<i32>().unwrap_or(-1))
      .collect();
    let older_than_2_12 = version_parts.len() > 1
      && (version_parts[0] < 2 || version_parts[0] == 2 && version_parts[1] < 12);
    println!("cargo::rustc-check-cfg=cfg(libxml_older_than_2_12)");
    if older_than_2_12 {
      println!("cargo::rustc-cfg=libxml_older_than_2_12");
    }
  } else {
    // otherwise, use the default bindings on platforms where pkg-config isn't available
    fs::copy(PathBuf::from("src/default_bindings.rs"), bindings_path)
      .expect("Failed to copy the default bindings to the build directory");
    // for now, assume that the library is older than 2.12, because that's what those bindings are computed with
    println!("cargo::rustc-cfg=libxml_older_than_2_12");
  }
}

#[cfg(all(target_family = "windows", target_env = "msvc"))]
mod vcpkg_dep {
  use crate::ProbedLib;
  pub fn vcpkg_find_libxml2() -> Option<ProbedLib> {
    if let Ok(metadata) = vcpkg::Config::new().find_package("libxml2") {
      Some(ProbedLib {
        version: vcpkg_version(),
        include_paths: metadata.include_paths,
        clang_args: vec![],
      })
    } else {
      None
    }
  }

  fn vcpkg_version() -> String {
    // What is the best way to obtain the version on Windows *before* bindgen runs?
    // here we attempt asking the shell for "vcpkg list libxml2"
    let mut vcpkg_exe = vcpkg::find_vcpkg_root(&vcpkg::Config::new()).unwrap();
    vcpkg_exe.push("vcpkg.exe");
    let vcpkg_list_libxml2 = std::process::Command::new(vcpkg_exe)
      .args(["list", "libxml2"])
      .output()
      .expect("vcpkg.exe failed to execute in vcpkg_dep build step");
    if vcpkg_list_libxml2.status.success() {
      let libxml2_list_str = String::from_utf8_lossy(&vcpkg_list_libxml2.stdout);
      for line in libxml2_list_str.lines() {
        if line.starts_with("libxml2:") {
          let mut version_piece = line.split("2.");
          version_piece.next();
          if let Some(version_tail) = version_piece.next()
            && let Some(version) = version_tail.split(' ').next().unwrap().split('#').next()
          {
            return format!("2.{version}");
          }
        }
      }
    }
    // default to a recent libxml2 from Windows 10
    // (or should this panic?)
    String::from("2.13.5")
  }
}
