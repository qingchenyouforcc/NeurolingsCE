//! 基于 mascot_pack/ 真实桌宠夹具的集成测试。

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use neurolings_pack::{
    PackError, analyze_legacy_archive, extract_package, import_archive, inspect_package,
    install_package, validate_package, write_legacy_archive_selection_as_packages,
    write_package_from_directory,
};

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../mascot_pack");

/// 构造合成包用的最小合法 1x1 RGBA PNG。
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

const FIXTURES: &[(&str, &str)] = &[
    ("Cerber", "Cerber"),
    ("Eviling", "Eviling"),
    ("Neuron", "Neuron"),
    ("Tuteling", "Tuteling"),
    ("Vedaling", "Vedaling"),
    ("Weuron", "Weuron"),
];

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(FIXTURE_DIR).join(format!("{name}.mascot"))
}

#[test]
fn validates_every_fixture_package() {
    for (file, expected_name) in FIXTURES {
        let report = validate_package(&fixture_path(file));
        assert!(report.ok, "{file}: errors = {:?}", report.errors);
        assert_eq!(report.metadata.name, *expected_name, "{file}");
        assert!(report.entry_count > 0);
        assert!(report.file_count > 0);
        assert!(report.extracted_bytes > 0);
        assert!(report.errors.is_empty());
    }
}

#[test]
fn inspects_every_fixture_package() {
    for (file, expected_name) in FIXTURES {
        let metadata = inspect_package(&fixture_path(file)).expect("inspect should succeed");
        assert_eq!(metadata.name, *expected_name);
    }
}

#[test]
fn extracts_fixture_package() {
    let temp = tempfile::tempdir().unwrap();
    extract_package(&fixture_path("Cerber"), temp.path()).expect("extraction should succeed");
    assert!(temp.path().join("info.json").is_file());
    assert!(temp.path().join("actions.xml").is_file());
    assert!(temp.path().join("behaviors.xml").is_file());
    assert!(temp.path().join("img").is_dir());
}

#[test]
fn rejects_missing_package() {
    let missing = Path::new(FIXTURE_DIR).join("NoSuch.mascot");
    let report = validate_package(&missing);
    assert!(!report.ok);
    assert_eq!(report.errors, vec!["Mascot package does not exist"]);
}

/// 构建一个小而完整的桌宠源目录。
fn write_mascot_source(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir.join("img")).unwrap();
    std::fs::write(
        dir.join("info.json"),
        format!("{{\n    \"name\": \"{name}\",\n    \"version\": \"1.0\"\n}}"),
    )
    .unwrap();
    std::fs::write(dir.join("actions.xml"), "<Mascot><ActionList/></Mascot>").unwrap();
    std::fs::write(
        dir.join("behaviors.xml"),
        "<Mascot><BehaviorList/></Mascot>",
    )
    .unwrap();
    std::fs::write(dir.join("img/shime1.png"), TINY_PNG).unwrap();
}

#[test]
fn rejects_zip_slip_entries() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    write_mascot_source(&source, "Slippery");
    let package_path = temp.path().join("Slippery.mascot");
    write_package_from_directory(&source, &package_path).unwrap();

    // 重写包并注入恶意 `../` 条目。
    let malicious_path = temp.path().join("Malicious.mascot");
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&malicious_path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let mut original =
            zip::ZipArchive::new(std::fs::File::open(&package_path).unwrap()).unwrap();
        for index in 0..original.len() {
            let mut file = original.by_index(index).unwrap();
            if file.name().ends_with('/') {
                continue;
            }
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut data).unwrap();
            zip.start_file(file.name().to_string(), options).unwrap();
            zip.write_all(&data).unwrap();
        }
        zip.start_file("../evil.txt", options).unwrap();
        zip.write_all(b"pwned").unwrap();
        zip.start_file("img/../../evil2.png", options).unwrap();
        zip.write_all(b"pwned").unwrap();
        zip.finish().unwrap();
    }

    // 校验必须拒绝不安全条目。
    let report = validate_package(&malicious_path);
    assert!(!report.ok);
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.starts_with("Unsupported or unsafe package entry:")),
        "errors = {:?}",
        report.errors
    );

    // 解压绝不能写出输出目录之外。
    let output = temp.path().join("out");
    let _ = extract_package(&malicious_path, &output);
    assert!(!temp.path().join("evil.txt").exists());
    assert!(!temp.path().join("evil2.png").exists());
}

#[test]
fn round_trip_write_then_install() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    write_mascot_source(&source, "RoundTrip");

    let package_path = temp.path().join("RoundTrip.mascot");
    write_package_from_directory(&source, &package_path).unwrap();

    let report = validate_package(&package_path);
    assert!(report.ok, "errors = {:?}", report.errors);
    assert_eq!(report.metadata.name, "RoundTrip");

    let storage = temp.path().join("storage");
    let installed_name = install_package(&package_path, &storage).unwrap();
    assert_eq!(installed_name, "RoundTrip");
    assert!(storage.join("RoundTrip.mascot").is_file());

    let metadata = inspect_package(&storage.join("RoundTrip.mascot")).unwrap();
    assert_eq!(metadata.name, "RoundTrip");
    assert_eq!(metadata.version, "1.0");
}

#[test]
fn analyzes_legacy_zip_archive() {
    let archive = Path::new(FIXTURE_DIR).join("Cerber.zip");
    let analysis = analyze_legacy_archive(&archive);
    assert!(analysis.ok, "error = {}", analysis.error_message);
    assert!(!analysis.candidates.is_empty());
    let cerber = analysis
        .candidates
        .iter()
        .find(|candidate| candidate.name == "Cerber")
        .expect("Cerber candidate");
    assert!(cerber.convertible, "errors = {:?}", cerber.errors);
    assert_eq!(cerber.metadata.name, "Cerber");
}

#[test]
fn imports_legacy_zip_archive_into_storage() {
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().join("storage");
    let archive = Path::new(FIXTURE_DIR).join("Cerber.zip");

    let imported = import_archive(&archive, &storage).unwrap();
    assert_eq!(imported.len(), 1);
    assert!(imported.contains("Cerber"));

    let installed = storage.join("Cerber.mascot");
    assert!(installed.is_file());
    let report = validate_package(&installed);
    assert!(report.ok, "errors = {:?}", report.errors);
    assert_eq!(report.metadata.name, "Cerber");
}

#[test]
fn imports_mascot_package_directly() {
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().join("storage");
    let imported = import_archive(&fixture_path("Eviling"), &storage).unwrap();
    assert_eq!(imported.len(), 1);
    assert!(imported.contains("Eviling"));
    assert!(storage.join("Eviling.mascot").is_file());
}

#[test]
fn converts_legacy_selection_as_packages() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let archive = Path::new(FIXTURE_DIR).join("Cerber.zip");

    let results = write_legacy_archive_selection_as_packages(
        &archive,
        &output,
        &["Cerber".to_string()],
        &BTreeMap::new(),
    );
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(result.ok, "error = {}", result.error_message);
    assert_eq!(result.name, "Cerber");
    let report = validate_package(Path::new(&result.package_path));
    assert!(report.ok, "errors = {:?}", report.errors);
}

#[test]
fn unsupported_archive_formats_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let fake = temp.path().join("legacy.7z");
    std::fs::write(&fake, b"not really a 7z file").unwrap();

    let storage = temp.path().join("storage");
    let error = import_archive(&fake, &storage).unwrap_err();
    assert!(matches!(error, PackError::Unsupported(_)), "{error:?}");
    assert!(error.to_string().contains("7z"));

    let analysis = analyze_legacy_archive(&fake);
    assert!(!analysis.ok);
    assert!(analysis.error_message.contains("7z"));

    let fake_rar = temp.path().join("legacy.rar");
    std::fs::rename(&fake, &fake_rar).unwrap();
    let error = import_archive(&fake_rar, &storage).unwrap_err();
    assert!(matches!(error, PackError::Unsupported(_)));
}
