use std::{
  env, fs,
  path::{Path, PathBuf},
};

struct ProbedLib {
  version: String,
  include_paths: Vec<PathBuf>,
}

/// 检测目标平台
fn get_target_platform() -> String {
  env::var("TARGET").unwrap_or_else(|_| "unknown".to_string())
}

/// 检测是否为Android平台
fn is_android_target() -> bool {
  get_target_platform().contains("android")
}

/// 获取Android架构
fn get_android_arch() -> Option<String> {
  let target = get_target_platform();
  if target.contains("aarch64") {
    Some("arm64-v8a".to_string())
  } else if target.contains("armv7") {
    Some("armeabi-v7a".to_string())
  } else if target.contains("i686") {
    Some("x86".to_string())
  } else if target.contains("x86_64") {
    Some("x86_64".to_string())
  } else {
    None
  }
}

/// 处理Android交叉编译
fn handle_android_cross_compile() -> Option<ProbedLib> {
  let arch = get_android_arch()?;
  println!("cargo:rerun-if-env-changed=ANDROID_NDK_ROOT");
  println!("cargo:rerun-if-env-changed=LIBXML2_PREBUILT_PATH");
  // 检查是否有预构建的库
  if let Ok(prebuilt_path) = env::var("LIBXML2_PREBUILT_PATH") {
    let lib_path = PathBuf::from(&prebuilt_path).join(&arch);
    if lib_path.join("lib").join("libxml2.a").exists() {
      println!(
        "cargo:rustc-link-search=native={}",
        lib_path.join("lib").display()
      );
      println!("cargo:rustc-link-lib=static=xml2");

      // libxml2的头文件通常在include/libxml2目录中
      let mut include_paths = vec![lib_path.join("include")];
      let libxml2_include = lib_path.join("include").join("libxml2");
      if libxml2_include.exists() {
        include_paths.push(libxml2_include);
      }

      return Some(ProbedLib {
        version: "2.10.3".to_string(),
        include_paths,
      });
    }
  }

  // 如果没有预构建库，提示用户构建
  println!(
    "cargo:warning=No prebuilt libxml2 found for Android {}. Please run:",
    arch
  );
  println!("cargo:warning=  ./scripts/build_libxml2.sh --platform android --arch {} --ndk-path $ANDROID_NDK_ROOT", arch);
  println!("cargo:warning=  export LIBXML2_PREBUILT_PATH=./prebuilt/android-21");

  None
}

/// Finds libxml2 and optionally return a list of header
/// files from which the bindings can be generated.
fn find_libxml2() -> Option<ProbedLib> {
  #![allow(unreachable_code)] // for platform-dependent dead code

  // 首先检查交叉编译环境
  if is_android_target() {
    if let Some(lib) = handle_android_cross_compile() {
      return Some(lib);
    }
  }

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

fn generate_bindings(header_dirs: Vec<PathBuf>, output_path: &Path) {
  let mut builder = bindgen::Builder::default()
    .header("src/wrapper.h")
    .opaque_type("max_align_t")
    // invalidate build as soon as the wrapper changes
    .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
    .layout_tests(true)
    .clang_args(&["-DPKG-CONFIG"])
    .clang_args(header_dirs.iter().map(|dir| format!("-I{}", dir.display())));
  if is_android_target() {
    if let Ok(ndk_root) = env::var("ANDROID_NDK_ROOT") {
      let target = get_target_platform();
      let api_level = env::var("ANDROID_API_LEVEL").unwrap_or_else(|_| "21".to_string());

      println!("cargo:warning=Building for Android target: {}", target);
      println!("cargo:warning=Using NDK: {}", ndk_root);
      println!("cargo:warning=API level: {}", api_level);

      // 检测主机操作系统
      let host_os = if cfg!(target_os = "linux") {
        "linux-x86_64"
      } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
      } else if cfg!(target_os = "windows") {
        "windows-x86_64"
      } else {
        "linux-x86_64" // 默认
      };

      // 设置sysroot和target
      let sysroot = format!("{}/toolchains/llvm/prebuilt/{}/sysroot", ndk_root, host_os);

      // 构建正确的target triple
      let clang_target = if target.contains("aarch64") {
        format!("aarch64-linux-android{}", api_level)
      } else if target.contains("armv7") {
        format!("armv7a-linux-androideabi{}", api_level)
      } else if target.contains("i686") {
        format!("i686-linux-android{}", api_level)
      } else if target.contains("x86_64") {
        format!("x86_64-linux-android{}", api_level)
      } else {
        format!("{}{}", target, api_level)
      };

      // 添加Android特定的系统头文件路径
      let usr_include = format!("{}/usr/include", sysroot);
      let arch_include = if target.contains("aarch64") {
        format!("{}/usr/include/aarch64-linux-android", sysroot)
      } else if target.contains("armv7") {
        format!("{}/usr/include/arm-linux-androideabi", sysroot)
      } else if target.contains("i686") {
        format!("{}/usr/include/i686-linux-android", sysroot)
      } else if target.contains("x86_64") {
        format!("{}/usr/include/x86_64-linux-android", sysroot)
      } else {
        usr_include.clone()
      };

      println!("cargo:warning=Using host OS: {}", host_os);
      println!("cargo:warning=Using sysroot: {}", sysroot);
      println!("cargo:warning=Using clang target: {}", clang_target);
      println!(
        "cargo:warning=Using include paths: {} and {}",
        usr_include, arch_include
      );

      builder = builder.clang_args(&[
        "--sysroot",
        &sysroot,
        "-target",
        &clang_target,
        &format!("-I{}", usr_include),
        &format!("-I{}", arch_include),
        "-D__ANDROID_API__=21",
        "-D__ANDROID__",
      ]);
    }
  }

  builder
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
    generate_bindings(probed_lib.include_paths, &bindings_path);
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
          if let Some(version_tail) = version_piece.next() {
            if let Some(version) = version_tail.split(' ').next().unwrap().split('#').next() {
              return format!("2.{version}");
            }
          }
        }
      }
    }
    // default to a recent libxml2 from Windows 10
    // (or should this panic?)
    String::from("2.13.5")
  }
}
