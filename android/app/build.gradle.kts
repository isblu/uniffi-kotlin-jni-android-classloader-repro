plugins {
  id("com.android.application")
}

// Same Rust/NDK wiring as chatr (apps/mobile/app/build.gradle.kts), reduced to one crate.
val repoRoot = rootProject.projectDir.resolve("..").canonicalFile
val rustCrateDir = repoRoot.resolve("rust")
val uniffiOutDir = layout.buildDirectory.dir("generated/uniffi")
val rustNdkVersion = "27.1.12297006"
val abis = listOf("arm64-v8a") + (providers.gradleProperty("reproExtraAbis").orNull
  ?.split(",")?.map(String::trim)?.filter(String::isNotEmpty) ?: emptyList())
val android16KbRustFlags = "-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384"

// flavor name -> cargo feature args
val rustFlavors = mapOf(
  "upstream" to listOf<String>(),
  "patched" to listOf("--no-default-features", "--features", "patched"),
)

fun cargoExecutable(): String {
  val home = System.getProperty("user.home")
  return listOf(
    providers.environmentVariable("CARGO_HOME").orNull?.let { "$it/bin/cargo" },
    "$home/.cargo/bin/cargo",
  ).filterNotNull().map(::file).firstOrNull(File::exists)?.absolutePath ?: "cargo"
}

fun androidNdkDirectory(): File {
  (providers.environmentVariable("ANDROID_NDK_HOME").orNull
    ?: providers.environmentVariable("ANDROID_NDK_ROOT").orNull)
    ?.let(::file)?.takeIf(File::isDirectory)?.let { return it }
  val sdk = providers.environmentVariable("ANDROID_HOME").orNull
    ?: providers.environmentVariable("ANDROID_SDK_ROOT").orNull
    ?: rootProject.file("local.properties").takeIf(File::isFile)?.readLines()
      ?.firstOrNull { it.startsWith("sdk.dir=") }?.removePrefix("sdk.dir=")
  sdk?.let { file("$it/ndk/$rustNdkVersion") }?.takeIf(File::isDirectory)?.let { return it }
  throw GradleException("Android NDK $rustNdkVersion is required. Install it or set ANDROID_NDK_HOME.")
}

android {
  namespace = "dev.isblu.uniffijnirepro"
  compileSdk {
    version = release(37) {
      minorApiLevel = 2
    }
  }
  ndkVersion = rustNdkVersion

  defaultConfig {
    applicationId = "dev.isblu.uniffijnirepro"
    minSdk = 28
    targetSdk = 37
    versionCode = 1
    versionName = "0.1"
    ndk {
      abiFilters += abis
    }
  }

  flavorDimensions += "runtime"
  productFlavors {
    create("upstream") {
      dimension = "runtime"
      resValue("string", "app_name", "UniFFI JNI Repro")
    }
    create("patched") {
      dimension = "runtime"
      applicationIdSuffix = ".patched"
      resValue("string", "app_name", "UniFFI JNI Repro (patched)")
    }
  }

  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }
  buildFeatures {
    resValues = true
  }
  sourceSets {
    getByName("main") {
      kotlin.directories.add(uniffiOutDir.get().asFile.absolutePath)
    }
    rustFlavors.keys.forEach { flavor ->
      getByName(flavor) {
        jniLibs.directories.add(layout.buildDirectory.dir("generated/rustJniLibs/$flavor").get().asFile.absolutePath)
      }
    }
  }
}

kotlin {
  jvmToolchain(17)
}

val rustInputs = fileTree(rustCrateDir.resolve("src")) + files(
  rustCrateDir.resolve("Cargo.toml"),
  rustCrateDir.resolve("Cargo.lock"),
  rustCrateDir.resolve("build.rs"),
  rustCrateDir.resolve("uniffi.toml"),
)

rustFlavors.forEach { (flavor, featureArgs) ->
  val cap = flavor.replaceFirstChar(Char::uppercase)
  val task = tasks.register<Exec>("buildRust$cap") {
    val outputDirectory = layout.buildDirectory.dir("generated/rustJniLibs/$flavor").get().asFile
    doFirst { outputDirectory.deleteRecursively() }
    inputs.files(rustInputs)
    inputs.files(fileTree(repoRoot.resolve("vendor")))
    inputs.property("abis", abis)
    inputs.property("rustflags", android16KbRustFlags)
    outputs.dir(outputDirectory)
    workingDir(rustCrateDir)
    environment("ANDROID_NDK_HOME", androidNdkDirectory())
    environment("ANDROID_NDK_ROOT", androidNdkDirectory())
    // Separate target dirs so both flavors can build without clobbering each other's .so.
    environment("CARGO_TARGET_DIR", rustCrateDir.resolve("target/android-$flavor").absolutePath)
    environment("CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS", android16KbRustFlags)
    environment("CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS", android16KbRustFlags)
    commandLine(
      cargoExecutable(), "ndk",
      *abis.flatMap { listOf("-t", it) }.toTypedArray(),
      "-o", outputDirectory.absolutePath,
      "build", "--release", "-j", "4",
      *featureArgs.toTypedArray(),
    )
  }
  tasks.matching { it.name == "pre${cap}DebugBuild" || it.name == "pre${cap}ReleaseBuild" }.configureEach {
    dependsOn(task)
  }
}

// uniffi_parse_rs: bindings come straight from the Rust sources (`src:repro`), no host .so needed.
val generateUniffiKotlin = tasks.register<Exec>("generateUniffiKotlin") {
  val outputDirectory = uniffiOutDir.get().asFile
  inputs.files(rustInputs)
  outputs.dir(outputDirectory)
  workingDir(rustCrateDir)
  doFirst { outputDirectory.deleteRecursively() }
  commandLine(
    cargoExecutable(), "run", "--quiet", "-j", "4",
    "--features", "cli", "--bin", "uniffi-bindgen",
    "--", "bindings", "src:repro", outputDirectory.absolutePath,
  )
}

tasks.named("preBuild") {
  dependsOn(generateUniffiKotlin)
}
