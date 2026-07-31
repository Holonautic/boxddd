use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const REVIEWED_MODULES: &str = include_str!("fixtures/ffi_lifetime_inventory.txt");
const ACTIVITY_CLASSES: &[&str] = &[
    "attachment-or-storage-cleanup",
    "caller-scoped",
    "foundation-bootstrap",
    "ordinary-owner",
    "owner-native-callback",
    "replay-exclusive",
    "retained-owner-cleanup",
    "transient",
    "world-owner",
];

#[test]
fn native_call_modules_match_the_reviewed_inventory() {
    let expected = parse_reviewed_modules(REVIEWED_MODULES)
        .expect("the FFI module inventory should be well formed");
    let observed = collect_native_modules();

    assert_eq!(
        observed, expected,
        "modules that reference native functions changed; review the module's Foundation activity and update the inventory"
    );
}

#[test]
fn global_synchronization_and_raw_borrowing_remain_narrow() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut source_files = Vec::new();
    collect_rust_sources(&source_root, &mut source_files);

    let mut all_source = String::new();
    for path in source_files {
        all_source.push_str(&fs::read_to_string(path).expect("crate source should be readable"));
    }
    assert!(!all_source.contains("box3d_lock"));
    assert!(!all_source.contains("BOX3D_GLOBAL_LOCK"));
    assert!(!all_source.contains("MutexGuard<'static, ()>"));

    let foundation = fs::read_to_string(source_root.join("core/foundation.rs")).unwrap();
    assert_eq!(
        foundation.matches("static FOUNDATION_INIT_LOCK:").count(),
        1
    );
    assert_eq!(
        foundation
            .matches("static WORLD_SLOT_MUTATION_LOCK:")
            .count(),
        1
    );

    let raw = fs::read_to_string(source_root.join("raw.rs")).unwrap();
    assert!(raw.contains("world: &'a mut World"));
    assert!(raw.contains("_call: callback_state::OwnerCallFrame"));
    assert!(!raw.contains("Mutex"));
}

fn parse_reviewed_modules(input: &str) -> Result<BTreeSet<String>, String> {
    let allowed = ACTIVITY_CLASSES.iter().copied().collect::<BTreeSet<_>>();
    let mut modules = BTreeSet::new();

    for (index, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((path, activities)) = line.split_once(" | ") else {
            return Err(format!(
                "line {} must contain `path | activities`",
                index + 1
            ));
        };
        if path.is_empty() {
            return Err(format!("line {} has an empty module path", index + 1));
        }

        let activities = activities
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>();
        if activities.is_empty() {
            return Err(format!("line {} has no activity class", index + 1));
        }
        if let Some(activity) = activities
            .iter()
            .find(|activity| !allowed.contains(**activity))
        {
            return Err(format!(
                "line {} has unknown activity class `{activity}`",
                index + 1
            ));
        }
        if !modules.insert(path.to_owned()) {
            return Err(format!("line {} repeats module `{path}`", index + 1));
        }
    }

    Ok(modules)
}

fn collect_native_modules() -> BTreeSet<String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_root = manifest_dir.join("src");
    let functions = native_function_names(
        &manifest_dir
            .parent()
            .expect("boxddd should be inside the workspace")
            .join("boxddd-sys/src"),
    );
    let mut source_files = Vec::new();
    collect_rust_sources(&source_root, &mut source_files);

    source_files
        .into_iter()
        .filter_map(|path| {
            let source = fs::read_to_string(&path).expect("crate source should be readable");
            let uses_native = functions
                .iter()
                .any(|function| contains_identifier(&source, &format!("ffi::{function}")))
                || source.contains("ffi::boxddd_provider_");
            uses_native.then(|| {
                path.strip_prefix(&source_root)
                    .expect("source path should be below src")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
        })
        .collect()
}

fn native_function_names(bindings_root: &Path) -> BTreeSet<String> {
    [
        "bindings_pregenerated.rs",
        "bindings_pregenerated_double.rs",
    ]
    .into_iter()
    .flat_map(|name| {
        let source = fs::read_to_string(bindings_root.join(name))
            .expect("pregenerated bindings should be readable");
        source
            .lines()
            .filter_map(|line| {
                line.trim_start()
                    .strip_prefix("pub fn ")
                    .and_then(|rest| rest.split('(').next())
                    .filter(|name| !name.is_empty())
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>()
    })
    .collect()
}

fn contains_identifier(source: &str, identifier: &str) -> bool {
    source.match_indices(identifier).any(|(start, _)| {
        let before = source[..start].chars().next_back();
        let after = source[start + identifier.len()..].chars().next();
        !before.is_some_and(is_identifier_char) && !after.is_some_and(is_identifier_char)
    })
}

fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn collect_rust_sources(root: &Path, files: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(root)
        .expect("crate source directory should be readable")
        .map(|entry| entry.expect("source directory entry should be readable"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}
