use boxddd::{Hull, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn explicit_public_try_functions_are_allowlisted() {
    let _: fn(&Hull) -> Result<Hull> = Hull::try_clone;

    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut source_files = Vec::new();
    collect_rust_sources(&source_root, &mut source_files);
    source_files.sort();

    let mut public_try_functions = Vec::new();
    for path in &source_files {
        let source = fs::read_to_string(path).expect("crate source should be readable");
        for (line_index, line) in source.lines().enumerate() {
            let Some(name) = explicit_public_try_function(line) else {
                continue;
            };
            let relative = path
                .strip_prefix(&source_root)
                .expect("source file should be below crate source root");
            public_try_functions.push((relative.to_path_buf(), name.to_owned(), line_index + 1));
        }
    }

    let allowlisted: Vec<_> = public_try_functions
        .iter()
        .map(|(path, name, _)| (path.as_path(), name.as_str()))
        .collect();
    assert_eq!(
        allowlisted,
        [(Path::new("shapes.rs"), "try_clone")],
        "explicit public try_* functions must be reviewed and allowlisted: {public_try_functions:#?}"
    );
}

#[test]
fn legacy_api_error_aliases_are_absent() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut source_files = Vec::new();
    collect_rust_sources(&source_root, &mut source_files);

    for path in source_files {
        let source = fs::read_to_string(&path).expect("crate source should be readable");
        assert!(
            !source.contains("ApiError") && !source.contains("ApiResult"),
            "legacy error aliases must not reappear in {}",
            path.display()
        );
    }
}

fn explicit_public_try_function(line: &str) -> Option<&str> {
    let mut declaration = line.split("//").next()?.trim_start().strip_prefix("pub ")?;
    loop {
        if let Some(rest) = declaration.strip_prefix("const ") {
            declaration = rest;
        } else if let Some(rest) = declaration.strip_prefix("async ") {
            declaration = rest;
        } else if let Some(rest) = declaration.strip_prefix("unsafe ") {
            declaration = rest;
        } else if let Some(rest) = declaration.strip_prefix("extern ") {
            declaration = rest.trim_start();
            if declaration.starts_with('"') {
                let closing_quote = declaration[1..].find('"')? + 2;
                declaration = declaration[closing_quote..].trim_start();
            }
        } else {
            break;
        }
    }

    let name = declaration.strip_prefix("fn ")?.split(['(', '<']).next()?;
    name.starts_with("try_").then_some(name)
}

fn collect_rust_sources(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("crate source directory should be readable") {
        let path = entry
            .expect("source directory entry should be readable")
            .path();
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}
