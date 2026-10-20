use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const BACKENDS: &[Backend] = &[Backend::Opt, Backend::Avx2, Backend::Neon];
const VARIANTS: &[&str] = &["MAYO_1", "MAYO_2", "MAYO_3", "MAYO_5"];
const STATIC_LIBRARIES: &[&str] = &[
    "mayo_1_nistapi",
    "mayo_2_nistapi",
    "mayo_3_nistapi",
    "mayo_5_nistapi",
    "mayo_1",
    "mayo_2",
    "mayo_3",
    "mayo_5",
    "mayo_common_sys",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Backend {
    Opt,
    Avx2,
    Neon,
}

fn main() {
    rerun_if_changed();
    register_cfgs();

    let backend = select_backend();
    println!("cargo:rustc-cfg=mayo_backend_{}", backend.name());

    let manifest_dir = manifest_dir();
    let source_dir = manifest_dir.join("MAYO-C");
    let bridge_source = manifest_dir.join("src").join("bridge.c");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let build_dir = out_dir.join(format!("mayo-c-{}", backend.name()));

    assert_submodule_exists(&source_dir);
    build_mayo_c(&source_dir, &build_dir, backend);
    build_bridge(&source_dir, &bridge_source, &out_dir, backend);
    link_static_libraries(&build_dir);
}

fn rerun_if_changed() {
    for path in [
        "build.rs",
        "src/bridge.c",
        "MAYO-C/CMakeLists.txt",
        "MAYO-C/.cmake",
        "MAYO-C/apps",
        "MAYO-C/include",
        "MAYO-C/src",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }

    for variable in [
        "CARGO_FEATURE_OPT",
        "CARGO_FEATURE_AVX2",
        "CARGO_FEATURE_NEON",
        "CARGO_CFG_TARGET_ARCH",
        "CARGO_CFG_TARGET_ENV",
        "CARGO_CFG_TARGET_FEATURE",
        "CARGO_CFG_TARGET_VENDOR",
        "CARGO_CFG_UNIX",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
}

fn register_cfgs() {
    for backend in BACKENDS {
        println!("cargo:rustc-check-cfg=cfg(mayo_backend_{})", backend.name());
    }
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
}

fn assert_submodule_exists(source_dir: &Path) {
    if source_dir.join("CMakeLists.txt").exists() {
        return;
    }

    panic!("MAYO-C submodule is missing; run `git submodule update --init --recursive`");
}

fn select_backend() -> Backend {
    let explicit = BACKENDS
        .iter()
        .copied()
        .filter(|backend| backend.feature_enabled())
        .collect::<Vec<_>>();

    match explicit.as_slice() {
        [] => detect_backend(),
        [backend] => *backend,
        _ => {
            println!(
                "cargo:warning=multiple MAYO backend features enabled; falling back to platform auto-detection"
            );
            detect_backend()
        }
    }
}

fn detect_backend() -> Backend {
    match env::var("CARGO_CFG_TARGET_ARCH")
        .unwrap_or_default()
        .as_str()
    {
        "x86_64" if has_target_feature("avx2") => Backend::Avx2,
        "aarch64" | "arm" if has_target_feature("neon") => Backend::Neon,
        _ => Backend::Opt,
    }
}

fn has_target_feature(feature: &str) -> bool {
    env::var("CARGO_CFG_TARGET_FEATURE")
        .unwrap_or_default()
        .split(',')
        .any(|target_feature| target_feature == feature)
}

fn msvc() -> bool {
    env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
}

fn build_mayo_c(source_dir: &Path, build_dir: &Path, backend: Backend) {
    reset_cmake_build_dir(build_dir);

    let mut cmake = Command::new("cmake");
    cmake
        .arg("-S")
        .arg(source_dir)
        .arg("-B")
        .arg(build_dir)
        .arg("-DBUILD_SHARED_LIBS=OFF")
        .arg("-DENABLE_TESTS=OFF")
        .arg("-DENABLE_STRICT=OFF")
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg(format!("-DMAYO_BUILD_TYPE={}", backend.name()));

    if msvc() {
        // MAYO-C sets CMAKE_C_STANDARD to 99 with a plain set(), which shadows a
        // -DCMAKE_C_STANDARD cache override. MSVC has no C99 mode, so CMake emits no
        // /std flag and <stdalign.h> does not define `alignas` in legacy C mode.
        // Force C17 through CMAKE_C_FLAGS instead.
        cmake.arg("-DCMAKE_C_FLAGS=/std:c17");
    }

    configure_cmake_backend(&mut cmake, backend);
    run(&mut cmake);

    run(Command::new("cmake")
        .arg("--build")
        .arg(build_dir)
        .arg("--config")
        .arg("Release"));
}

fn reset_cmake_build_dir(build_dir: &Path) {
    if !build_dir.exists() {
        return;
    }

    fs::remove_dir_all(build_dir)
        .unwrap_or_else(|error| panic!("failed to remove {}: {error}", build_dir.display()));
}

fn configure_cmake_backend(cmake: &mut Command, backend: Backend) {
    if backend == Backend::Avx2 && has_target_feature("aes") {
        cmake.arg("-DENABLE_AESNI=ON");
    }

    if backend == Backend::Neon && has_target_feature("aes") {
        cmake.arg("-DENABLE_AESNEON=ON");
    }
}

fn build_bridge(source_dir: &Path, bridge_source: &Path, out_dir: &Path, backend: Backend) {
    let bridge_include = bridge_source.to_string_lossy().replace('\\', "\\\\");

    for variant in VARIANTS {
        let wrapper = out_dir.join(format!("mayo_bridge_{variant}.c"));
        fs::write(
            &wrapper,
            format!(
                "#define MAYO_VARIANT {variant}\n\
                 #include \"{bridge_include}\"\n"
            ),
        )
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", wrapper.display()));

        let mut build = cc::Build::new();
        build
            .file(&wrapper)
            // The bridge includes mayo.c, which uses `alignas` from <stdalign.h>.
            // MSVC only provides it from C11, so C99 is reserved for other compilers.
            .std(if msvc() { "c17" } else { "c99" })
            .warnings(false)
            .opt_level(3)
            .flag_if_supported("-funroll-loops")
            .include(source_dir.join("include"))
            .include(source_dir.join("src"))
            .include(source_dir.join("src/common"));

        configure_bridge_backend(&mut build, source_dir, backend);
        configure_bridge_target(&mut build);
        build.compile(&format!("mayo_bridge_{variant}"));
    }
}

fn configure_bridge_backend(build: &mut cc::Build, source_dir: &Path, backend: Backend) {
    match backend {
        Backend::Opt => {
            build
                .define("MAYO_BUILD_TYPE_OPT", None)
                .include(source_dir.join("src/generic"));
        }
        Backend::Avx2 => {
            build
                .define("MAYO_BUILD_TYPE_AVX2", None)
                .define("MAYO_AVX", None)
                .flag_if_supported("-mavx2")
                .include(source_dir.join("src/AVX2"))
                .include(source_dir.join("src/generic"));

            if has_target_feature("aes") {
                build.define("ENABLE_AESNI", None);
            }
        }
        Backend::Neon => {
            build
                .define("MAYO_BUILD_TYPE_NEON", None)
                .define("MAYO_NEON", None)
                .include(source_dir.join("src/neon"))
                .include(source_dir.join("src/generic"));

            if has_target_feature("aes") {
                build.define("ENABLE_AESNEON", None);
            }
        }
    };
}

fn configure_bridge_target(build: &mut cc::Build) {
    match env::var("CARGO_CFG_TARGET_ARCH")
        .unwrap_or_default()
        .as_str()
    {
        "aarch64" => {
            build.define("TARGET_ARM64", None);
        }
        "arm" => {
            build.define("TARGET_ARM", None);
        }
        "x86_64" => {
            build.define("TARGET_AMD64", None);
        }
        "x86" => {
            build.define("TARGET_X86", None);
        }
        "s390x" => {
            build
                .define("TARGET_S390X", None)
                .define("TARGET_BIG_ENDIAN", None);
        }
        _ => {
            build.define("TARGET_OTHER", None);
        }
    };

    if env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple") {
        build.define("TARGET_OS_MAC", None);
    } else if env::var_os("CARGO_CFG_UNIX").is_some() {
        build.define("TARGET_OS_UNIX", None);
    } else {
        build.define("TARGET_OS_OTHER", None);
    }
}

fn link_static_libraries(build_dir: &Path) {
    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join("src").display()
    );

    // Multi-config CMake generators (Visual Studio on Windows) place outputs in a
    // per-config subdirectory instead of src itself.
    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join("src").join("Release").display()
    );

    for library in STATIC_LIBRARIES {
        println!("cargo:rustc-link-lib=static={library}");
    }
}

fn run(command: &mut Command) {
    let status = command.status().unwrap_or_else(|error| {
        panic!("failed to run {command:?}: {error}");
    });

    if status.success() {
        return;
    }

    panic!("{command:?} failed with {status}");
}

impl Backend {
    fn name(self) -> &'static str {
        match self {
            Self::Opt => "opt",
            Self::Avx2 => "avx2",
            Self::Neon => "neon",
        }
    }

    fn feature_enabled(self) -> bool {
        let feature = match self {
            Self::Opt => "CARGO_FEATURE_OPT",
            Self::Avx2 => "CARGO_FEATURE_AVX2",
            Self::Neon => "CARGO_FEATURE_NEON",
        };

        env::var_os(feature).is_some()
    }
}
