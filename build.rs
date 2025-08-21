// build.rs

use std::{
  env, fs,
  path::{Path, PathBuf},
  process::Command,
};

/// Represents the information of a found libxml2 library.
struct ProbedLib {
  /// Library version string.
  version: String,
  /// Header search paths.
  include_paths: Vec<PathBuf>,
  /// Extra clang arguments to pass to bindgen.
  clang_args: Vec<String>,
}

fn main() {
  let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));
  let bindings_path = out_dir.join("bindings.rs");

  // Pre-declare the custom cfg flag to inform Cargo about its existence.
  println!("cargo:rustc-check-cfg=cfg(libxml_older_than_2_12)");
  // Rerun this script if environment variables or source files change.
  println!("cargo:rerun-if-env-changed=LIBXML2");
  println!("cargo:rerun-if-changed=src/wrapper.h");
  println!("cargo:rerun-if-changed=src/default_bindings.rs");

  if let Some(probed_lib) = find_libxml2() {
    // If a library is found, generate fresh bindings from its headers.
    generate_bindings(
      &probed_lib.include_paths,
      &probed_lib.clang_args,
      &bindings_path,
    );

    // Expose the library version to the code for conditional compilation.
    // Parse the version string, e.g., "2.13.5" -> [2, 13, 5].
    let version_parts: Vec<u32> = probed_lib
      .version
      .split('.')
      .map(|part| part.parse().unwrap_or(0))
      .collect();

    // Check if the version is older than 2.12.
    if version_parts.len() >= 2
      && (version_parts[0] < 2 || (version_parts[0] == 2 && version_parts[1] < 12))
    {
      println!("cargo:rustc-cfg=libxml_older_than_2_12");
    }
  } else {
    // If the library is not found (e.g., on MSVC without pkg-config), use pre-generated default bindings.
    fs::copy("src/default_bindings.rs", bindings_path)
      .expect("Failed to copy the default bindings to the build directory");
    // The default bindings are based on an older version, so we assume it's older than 2.12.
    println!("cargo:rustc-cfg=libxml_older_than_2_12");
  }
}

/// Finds the libxml2 library and returns its metadata.
///
/// The search strategy priority is:
/// 1. `LIBXML2` environment variable (all platforms).
/// 2. Platform-specific search:
///    - Android: Build from source using the NDK.
///    - iOS: Use the library from the Xcode SDK.
///    - Windows (MSVC): Use vcpkg.
///    - Unix-like (including Windows GNU): Use pkg-config.
fn find_libxml2() -> Option<ProbedLib> {
  // 1. First, check the `LIBXML2` environment variable.
  if let Ok(lib_path_str) = env::var("LIBXML2") {
    let lib_path = PathBuf::from(lib_path_str);
    if !lib_path.is_file() {
      panic!(
        "LIBXML2 environment variable points to a non-file path: {}",
        lib_path.display()
      );
    }

    let lib_dir = lib_path
      .parent()
      .unwrap_or_else(|| Path::new("/"))
      .to_string_lossy();
    let lib_name = lib_path
      .file_stem()
      .and_then(|s| s.to_str())
      .and_then(|s| s.strip_prefix("lib"))
      .unwrap_or_else(|| {
        panic!(
          "Could not determine library name from LIBXML2 path: {}",
          lib_path.display()
        )
      });

    println!("cargo:rustc-link-search={}", lib_dir);
    println!("cargo:rustc-link-lib={}", lib_name);

    // When using the `LIBXML2` env var, we can't easily determine the version and include paths,
    // so we return `None` to use the pre-generated bindings.
    // The user must ensure headers are in the system path.
    return None;
  }

  // 2. Otherwise, perform a platform-specific search.
  let target = env::var("TARGET").expect("TARGET environment variable not set");

  if target.contains("android") {
    return find_libxml2_for_android(&target);
  }

  if target.contains("apple-ios") {
    return find_libxml2_for_ios(&target);
  }

  // For non-Android and non-iOS platforms, dispatch using cfg attributes.
  find_libxml2_via_pkgmgr()
}

#[cfg(any(
  target_family = "unix",
  target_os = "macos",
  all(target_family = "windows", target_env = "gnu")
))]
fn find_libxml2_via_pkgmgr() -> Option<ProbedLib> {
  // For Unix-like systems and Windows GNU, use pkg-config.
  match pkg_config::Config::new().probe("libxml-2.0") {
    Ok(lib) => Some(ProbedLib {
      include_paths: lib.include_paths,
      version: lib.version,
      clang_args: Vec::new(),
    }),
    Err(e) => {
      panic!("Could not find libxml2 using pkg-config: {}", e);
    }
  }
}

#[cfg(all(target_family = "windows", target_env = "msvc"))]
mod vcpkg_dep {
  use super::ProbedLib;
  use std::process::Command;

  // Finds libxml2 via vcpkg.
  pub fn find_libxml2() -> Option<ProbedLib> {
    vcpkg::Config::new()
      .find_package("libxml2")
      .map(|metadata| ProbedLib {
        version: get_vcpkg_package_version("libxml2").unwrap_or_else(|| "2.13.5".to_string()),
        include_paths: metadata.include_paths,
        clang_args: vec![],
      })
      .ok()
  }

  // Gets the package version by calling the `vcpkg list` command.
  fn get_vcpkg_package_version(pkg_name: &str) -> Option<String> {
    let vcpkg_root = vcpkg::find_vcpkg_root(&vcpkg::Config::new()).ok()?;
    let vcpkg_exe = vcpkg_root.join("vcpkg.exe");

    let output = Command::new(vcpkg_exe)
      .args(["list", pkg_name])
      .output()
      .ok()?;

    if !output.status.success() {
      return None;
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    // vcpkg output format is like: `libxml2:x64-windows         2.13.5#1         GNOME's XML parser and toolkit`.
    // We need to extract `2.13.5` from this.
    for line in output_str.lines() {
      if line.starts_with(pkg_name) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() > 2 {
          return parts[1].split('#').next().map(String::from);
        }
      }
    }
    None
  }
}

#[cfg(all(target_family = "windows", target_env = "msvc"))]
fn find_libxml2_via_pkgmgr() -> Option<ProbedLib> {
  // For Windows MSVC, use vcpkg.
  if let Some(lib) = vcpkg_dep::find_libxml2() {
    return Some(lib);
  }
  eprintln!("Could not find libxml2 via vcpkg. Please install it using: `vcpkg install libxml2`");
  None
}

#[cfg(not(any(
  target_family = "unix",
  target_os = "macos",
  all(target_family = "windows", target_env = "gnu"),
  all(target_family = "windows", target_env = "msvc")
)))]
fn find_libxml2_via_pkgmgr() -> Option<ProbedLib> {
  // For other unsupported platforms, fail the build.
  panic!("Unsupported platform: Could not find a suitable method to locate libxml2.");
}

/// Finds libxml2 for iOS.
fn find_libxml2_for_ios(target: &str) -> Option<ProbedLib> {
  // iOS builds are only supported on macOS hosts.
  if !cfg!(target_os = "macos") {
    panic!("iOS builds are only supported on macOS hosts");
  }

  let sdk = if target.contains("-sim") {
    "iphonesimulator"
  } else {
    "iphoneos"
  };

  let sdk_path = xcrun_sdk_path(sdk)
    .unwrap_or_else(|| panic!("Failed to resolve iOS SDK path for '{}' via xcrun", sdk));
  let include_dir = sdk_path.join("usr/include/libxml2");
  let lib_dir = sdk_path.join("usr/lib");

  println!("cargo:rustc-link-search=native={}", lib_dir.display());
  println!("cargo:rustc-link-lib=xml2");

  let clang_target = match target {
    "aarch64-apple-ios" => "arm64-apple-ios".to_string(),
    "aarch64-apple-ios-sim" => "arm64-apple-ios-simulator".to_string(),
    t if t.contains("-sim") => t.replace("-apple-ios-sim", "-apple-ios-simulator"),
    _ => target.to_string(),
  };

  let clang_args = vec![
    format!("--target={}", clang_target),
    "-isysroot".to_string(),
    sdk_path.display().to_string(),
    format!("-I{}", include_dir.display()),
  ];

  Some(ProbedLib {
    // It's not easy to get the version from the iOS SDK, so we hardcode a known compatible version here.
    version: "2.9.13".to_string(),
    include_paths: vec![include_dir],
    clang_args,
  })
}

/// Gets the iOS SDK path via `xcrun`.
fn xcrun_sdk_path(sdk: &str) -> Option<PathBuf> {
  let out = Command::new("xcrun")
    .args(["--sdk", sdk, "--show-sdk-path"])
    .output()
    .ok()?;

  if !out.status.success() {
    return None;
  }

  let binding = String::from_utf8_lossy(&out.stdout);
  let path_str = binding.trim();
  if path_str.is_empty() {
    None
  } else {
    Some(PathBuf::from(path_str))
  }
}

/// Finds and builds libxml2 for Android.
fn find_libxml2_for_android(target: &str) -> Option<ProbedLib> {
  println!("cargo:rerun-if-env-changed=ANDROID_NDK_ROOT");
  println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");

  let ndk_root = env::var("ANDROID_NDK_ROOT")
    .or_else(|_| env::var("ANDROID_NDK_HOME"))
    .map(PathBuf::from)
    .expect("Android target detected, but ANDROID_NDK_ROOT or ANDROID_NDK_HOME is not set.");

  // Ensure cmake is available.
  if which::which("cmake").is_err() {
    panic!("CMake not found. Please install CMake and ensure it is on your PATH.");
  }

  let api_level = env::var("ANDROID_PLATFORM")
    .ok()
    .and_then(|v| v.trim_start_matches("android-").parse::<u32>().ok())
    .unwrap_or(21);

  let (android_abi, clang_target) = map_android_target(target, api_level)
    .unwrap_or_else(|| panic!("Unsupported Android target triple: {}", target));

  let host_tag = get_ndk_host_tag();

  // auto set CC/CXX environment variables
  let bin_dir = ndk_root
    .join("toolchains/llvm/prebuilt")
    .join(host_tag)
    .join("bin");

  let cc_path = {
    let mut path = bin_dir.join(format!("{}-clang", clang_target));
    if cfg!(target_os = "windows") {
      path.set_extension("cmd");
    }
    path
  };

  let cxx_path = {
    let mut path = bin_dir.join(format!("{}-clang++", clang_target));
    if cfg!(target_os = "windows") {
      path.set_extension("cmd");
    }
    path
  };

  if cc_path.exists() && cxx_path.exists() {
    unsafe {
      env::set_var("CC", &cc_path);
      env::set_var("CXX", &cxx_path);
    }
  } else {
    // Provide a clearer error if the expected compilers are not found.
    panic!(
      "Could not find NDK compilers. Checked for: {} and {}",
      cc_path.display(),
      cxx_path.display()
    );
  }

  // Build libxml2.
  let (dst, include_dir) = build_libxml2_for_android(&ndk_root, android_abi, api_level);

  // Link against the static library.
  println!(
    "cargo:rustc-link-search=native={}",
    dst.join("lib").display()
  );
  println!("cargo:rustc-link-lib=static=xml2");

  // Configure clang arguments for bindgen.
  let sysroot = ndk_root
    .join("toolchains/llvm/prebuilt")
    .join(host_tag)
    .join("sysroot");
  let mut clang_args = vec![
    format!("--target={}", clang_target),
    "-D__ANDROID__".to_string(),
    format!("-I{}", include_dir.display()),
  ];
  if sysroot.exists() {
    clang_args.push(format!("--sysroot={}", sysroot.display()));
    let sys_inc = sysroot.join("usr/include");
    if sys_inc.exists() {
      clang_args.push(format!("-I{}", sys_inc.display()));
      // Add arch-specific include path.
      let arch_inc = sys_inc.join(map_clang_target_to_sysroot_arch(&clang_target));
      if arch_inc.exists() {
        clang_args.push(format!("-I{}", arch_inc.display()));
      }
    }
  }

  Some(ProbedLib {
    version: "2.13.5".to_string(), // Version from the source build.
    include_paths: vec![include_dir],
    clang_args,
  })
}

/// Maps a Rust Android target triple to an NDK ABI and a clang target string.
fn map_android_target(target: &str, api: u32) -> Option<(&'static str, String)> {
  let (abi, clang_triple) = match target {
    "aarch64-linux-android" => ("arm64-v8a", "aarch64-linux-android"),
    "armv7-linux-androideabi" => ("armeabi-v7a", "armv7a-linux-androideabi"),
    "i686-linux-android" => ("x86", "i686-linux-android"),
    "x86_64-linux-android" => ("x86_64", "x86_64-linux-android"),
    _ => return None,
  };
  Some((abi, format!("{}{}", clang_triple, api)))
}

/// Gets the NDK host tag for the current host OS (e.g., "linux-x86_64").
fn get_ndk_host_tag() -> &'static str {
  if cfg!(target_os = "linux") {
    "linux-x86_64"
  } else if cfg!(target_os = "windows") {
    "windows-x86_64"
  } else if cfg!(target_os = "macos") {
    if cfg!(target_arch = "aarch64") {
      "darwin-arm64"
    } else {
      "darwin-x86_64"
    }
  } else {
    panic!("Unsupported host OS for Android NDK");
  }
}

/// Maps a clang target to the corresponding architecture directory name in the NDK sysroot.
fn map_clang_target_to_sysroot_arch(clang_target: &str) -> &'static str {
  if clang_target.starts_with("aarch64-") {
    "aarch64-linux-android"
  } else if clang_target.starts_with("armv7") {
    "arm-linux-androideabi"
  } else if clang_target.starts_with("i686-") {
    "i686-linux-android"
  } else if clang_target.starts_with("x86_64-") {
    "x86_64-linux-android"
  } else {
    panic!("Unknown clang target for sysroot mapping: {}", clang_target);
  }
}

/// Builds libxml2 for Android using CMake and the NDK.
fn build_libxml2_for_android(ndk_root: &Path, abi: &str, api: u32) -> (PathBuf, PathBuf) {
  let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
  let src_dir = out_dir.join("libxml2-src");

  // If libxml2 is already built and the include directory exists, skip clone and build for efficiency.
  let build_dir = out_dir.join("build");
  let dst = out_dir.join("libxml2-build");
  let include_dir = dst.join("include").join("libxml2");

  if include_dir.exists() {
  // Already built, return the existing build and include directory.
    return (dst, include_dir);
  }

  // Clone the libxml2 repository if it does not exist locally.
  if !src_dir.exists() {
    if which::which("git").is_err() {
      panic!("Git not found. Please install git and ensure it is in your PATH.");
    }
    let repo_url = env::var("LIBXML2_GIT")
      .unwrap_or_else(|_| "https://github.com/GNOME/libxml2.git".to_string());
    let status = Command::new("git")
      .args([
        "clone",
        "--depth",
        "1",
        "--branch",
        "v2.13.5",
        &repo_url,
        src_dir.to_str().unwrap(),
      ])
      .status()
      .expect("Failed to execute git. Is it installed and in PATH?");
    if !status.success() {
      panic!("'git clone' of libxml2 failed with status: {}", status);
    }
  }

  // remove CMake cache
  if build_dir.exists() {
    let _ = fs::remove_file(build_dir.join("CMakeCache.txt"));
    let _ = fs::remove_dir_all(build_dir.join("CMakeFiles"));
  }

  // Configure CMake.
  let mut cfg = cmake::Config::new(&src_dir);
  cfg
    .profile("Release")
    .define(
      "CMAKE_TOOLCHAIN_FILE",
      ndk_root.join("build/cmake/android.toolchain.cmake"),
    )
    .define("ANDROID_ABI", abi)
    .define("ANDROID_PLATFORM", api.to_string())
    .define("BUILD_SHARED_LIBS", "OFF")
    // Trim features to reduce binary size and dependencies.
    .define("LIBXML2_WITH_PYTHON", "OFF")
    .define("LIBXML2_WITH_LZMA", "OFF")
    .define("LIBXML2_WITH_ZLIB", "OFF")
    .define("LIBXML2_WITH_ICONV", "OFF")
    .define("LIBXML2_WITH_TESTS", "OFF")
    .define("LIBXML2_WITH_PROGRAMS", "OFF");

  // Prefer using the Ninja generator.
  if let Ok(ninja_path) = which::which("ninja") {
    cfg.generator("Ninja");
    cfg.define("CMAKE_MAKE_PROGRAM", ninja_path);
  } else {
    #[cfg(target_os = "windows")]
    {
      panic!(
        "Ninja not found. On Windows, please install Ninja (e.g., `scoop install ninja`) for Android builds."
      );
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
      eprintln!("Warning: Ninja not detected. It is recommended to install Ninja (e.g., `sudo apt install ninja-build` or `brew install ninja`) for faster builds. Falling back to default CMake generator, which may be slower.");
    }
  }

  // Run the build.
  let build_result = cfg.build();
  // 为兼容原逻辑，build() 返回的路径可能不是 dst，重新赋值。
  let include_dir = build_result.join("include").join("libxml2");
  let dst = build_result;

  if !include_dir.exists() {
    panic!(
      "libxml2 include directory not found after build at {}",
      include_dir.display()
    );
  }

  (dst, include_dir)
}

/// Generates Rust bindings using bindgen.
fn generate_bindings(include_paths: &[PathBuf], extra_clang_args: &[String], output_path: &Path) {
  let mut builder = bindgen::Builder::default()
    .header("src/wrapper.h")
    .opaque_type("max_align_t") // Avoids generating an unstable definition for `max_align_t`.
    .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
    .layout_tests(true)
    .clang_args([
      "-DPKG-CONFIG",
      "-DLIBXML_C14N_ENABLED",
      "-DLIBXML_OUTPUT_ENABLED",
    ]);

  // Add include search paths.
  for path in include_paths {
    builder = builder.clang_arg(format!("-I{}", path.display()));
  }

  // Add platform-specific clang arguments.
  builder = builder.clang_args(extra_clang_args);

  builder
    .generate()
    .expect("Failed to generate bindings with bindgen")
    .write_to_file(output_path)
    .expect("Failed to write bindings to file");
}
