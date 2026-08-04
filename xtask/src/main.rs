use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[allow(dead_code)]
#[path = "../../boxddd-sys/src/upstream_contract.rs"]
mod upstream_contract;

#[allow(dead_code)]
#[path = "../../bevy_boxddd/examples/testbed_3d/scene_catalog.rs"]
mod scene_catalog;

use scene_catalog::{ParityMode, SCENE_CATALOG, SceneCatalogEntry};
use upstream_contract::{
    BOX3D_C_SOURCES, PROVIDER_ASSET_BASENAME, PROVIDER_BRIDGE_REVISION, PROVIDER_MODULE,
    UPSTREAM_COMMIT,
};

type DynError = Box<dyn Error>;
type Result<T> = std::result::Result<T, DynError>;

const TARGET: &str = "wasm32-unknown-unknown";
const PAGES_WASM_PROFILE_ENV: &str = "BOXDDD_PAGES_WASM_PROFILE";
const PAGES_WASM_OPT_ENV: &str = "BOXDDD_PAGES_WASM_OPT";
const SMOKE_PACKAGE: &str = "boxddd-provider-smoke";
const SMOKE_WASM: &str = "boxddd_provider_smoke.wasm";
const PAGES_WASM_DIR: &str = "wasm/generated";
const BEVY_EXAMPLES_DIR: &str = "examples";
const BEVY_WEB_EXAMPLE: &str = "testbed_3d";
const BEVY_WEB_OUT_DIR: &str = "bevy-testbed/generated";
const BEVY_WEB_OUT_NAME: &str = "bevy_boxddd_testbed";
const BEVY_WEB_JS: &str = "bevy_boxddd_testbed.js";
const BEVY_WEB_WASM: &str = "bevy_boxddd_testbed_bg.wasm";
const BEVY_PROVIDER_SHIM: &str = "box3d-provider-shim.js";
const SAMPLE_MATRIX_PATH: &str = "docs/upstream-parity/box3d-sample-matrix.md";
const SAMPLE_INVENTORY_PATH: &str = "docs/upstream-parity/box3d-sample-inventory.json";
const SAMPLE_CASE_TABLE_HEADER: &str =
    "| Category | Official sample | Source location | Parity mode | Target | Notes |";
const PROVIDER_SMOKE_EXPORTS: &[&str] = &[
    "boxddd_provider_smoke",
    "boxddd_provider_drop_millimeters",
    "boxddd_provider_ray_hit_millimeters",
    "boxddd_provider_shape_cast_permyriad",
    "boxddd_provider_joint_error_millimeters",
    "boxddd_provider_event_provenance_mask",
    "boxddd_provider_foundation_lifecycle_mask",
    "boxddd_provider_teardown_debug_shape_count",
];
const DEBUG_BRIDGE_EXPORTS: &[&str] = &[
    "boxddd_debug_shape_create",
    "boxddd_debug_shape_destroy",
    "boxddd_debug_draw_shape",
    "boxddd_debug_draw_segment",
    "boxddd_debug_draw_transform",
    "boxddd_debug_draw_point",
    "boxddd_debug_draw_sphere",
    "boxddd_debug_draw_capsule",
    "boxddd_debug_draw_bounds",
    "boxddd_debug_draw_box",
    "boxddd_debug_draw_string",
];
const QUERY_BRIDGE_EXPORTS: &[&str] = &["boxddd_query_overlap", "boxddd_query_cast"];

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
struct OfficialSample {
    category: String,
    name: String,
    source: String,
}

#[derive(Debug, Deserialize)]
struct OfficialSampleInventory {
    schema_version: u32,
    upstream_commit: String,
    samples: Vec<OfficialSample>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SampleParityRow {
    category: String,
    name: String,
    source: String,
    mode: SampleParityMode,
    target: String,
    notes: String,
    line_number: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SampleParityMode {
    FaithfulPort,
    TeachingAdaptation,
    TestOnly,
    Deferred,
    UpstreamReference,
}

impl SampleParityMode {
    const fn requires_target(self) -> bool {
        !matches!(self, Self::Deferred | Self::UpstreamReference)
    }

    const fn requires_test_target(self) -> bool {
        matches!(self, Self::TestOnly)
    }
}

impl TryFrom<&str> for SampleParityMode {
    type Error = ();

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value {
            "FaithfulPort" => Ok(Self::FaithfulPort),
            "TeachingAdaptation" => Ok(Self::TeachingAdaptation),
            "TestOnly" => Ok(Self::TestOnly),
            "Deferred" => Ok(Self::Deferred),
            "UpstreamReference" => Ok(Self::UpstreamReference),
            _ => Err(()),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum BuildProfile {
    Debug,
    Release,
    WasmRelease,
}

#[derive(Debug)]
struct BevyWebArtifacts {
    out_dir: PathBuf,
    imports: Vec<String>,
}

#[derive(Serialize)]
struct ProviderSmokeContract<'a> {
    provider_module: &'static str,
    provider_bridge_revision: u32,
    provider_file: &'a str,
    app_wasm_file: &'a str,
    shim_file: &'static str,
    provider_imports: &'a [String],
    required_app_exports: Vec<&'static str>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("xtask error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());
    match command.as_str() {
        "provider-smoke-app" => {
            let app = build_provider_smoke_app()?;
            let imports = collect_provider_imports(&app)?;
            write_exports_json(&provider_smoke_dir(), &imports)?;
            eprintln!(
                "Provider smoke app ready: {} ({} provider imports)",
                app.display(),
                imports.len()
            );
            Ok(())
        }
        "provider-smoke" => run_provider_smoke(),
        "build-pages-wasm" => build_pages_wasm(),
        "generate-pages" => generate_pages(),
        "validate-pages" => validate_pages(),
        "sample-parity" => sample_parity(args.collect()),
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown xtask command: {other}").into()),
    }
}

fn print_help() {
    eprintln!(
        "Commands:\n  provider-smoke-app   Build the Rust wasm provider-smoke app and export list\n  provider-smoke       Build the Rust app, build the Box3D provider with emcc, and run Node smoke\n  build-pages-wasm     Build Bevy example WASM artifacts into docs/pages/wasm/generated and docs/pages/bevy-testbed/generated\n  generate-pages       Generate static Bevy example entry pages from the Rust scene registry\n  validate-pages       Validate the static GitHub Pages site\n  sample-parity        Validate the official Box3D sample parity matrix\n\nEnvironment:\n  BOXDDD_PAGES_WASM_PROFILE=debug|release|wasm-release  Select the Rust profile for Pages wasm; default: wasm-release\n  BOXDDD_PAGES_WASM_OPT=0                                Disable optional wasm-opt -Oz post-processing"
    );
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should live under the workspace root")
        .to_path_buf()
}

fn provider_smoke_dir() -> PathBuf {
    project_root().join("target").join("boxddd-provider-smoke")
}

fn pages_wasm_generated_dir() -> PathBuf {
    project_root()
        .join("docs")
        .join("pages")
        .join(PAGES_WASM_DIR)
}

fn pages_bevy_generated_dir() -> PathBuf {
    project_root()
        .join("docs")
        .join("pages")
        .join(BEVY_WEB_OUT_DIR)
}

fn pages_bevy_examples_dir() -> PathBuf {
    project_root()
        .join("docs")
        .join("pages")
        .join(BEVY_EXAMPLES_DIR)
}

fn sample_parity(args: Vec<String>) -> Result<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        eprintln!("Usage: cargo run -p xtask -- sample-parity [--check]");
        return Ok(());
    }
    if !args.is_empty() && args != ["--check"] {
        return Err(format!("unknown sample-parity arguments: {}", args.join(" ")).into());
    }

    let root = project_root();
    let official_samples = read_official_samples(&root)?;
    let parity_rows = read_sample_parity_rows(&root)?;
    validate_sample_parity(&root, &official_samples, &parity_rows)?;

    eprintln!(
        "Validated official sample parity matrix: {} cases",
        official_samples.len()
    );
    Ok(())
}

fn read_official_samples(root: &Path) -> Result<Vec<OfficialSample>> {
    let path = root.join(SAMPLE_INVENTORY_PATH);
    let source = fs::read_to_string(&path)?;
    parse_official_sample_inventory(&path, &source)
}

fn parse_official_sample_inventory(path: &Path, source: &str) -> Result<Vec<OfficialSample>> {
    let inventory: OfficialSampleInventory = serde_json::from_str(source)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    if inventory.schema_version != 1 {
        return Err(format!(
            "{} uses unsupported schema version {}; expected 1",
            path.display(),
            inventory.schema_version
        )
        .into());
    }
    if inventory.upstream_commit != UPSTREAM_COMMIT {
        return Err(format!(
            "{} targets upstream commit {}, expected {}",
            path.display(),
            inventory.upstream_commit,
            UPSTREAM_COMMIT
        )
        .into());
    }
    if inventory.samples.is_empty() {
        return Err(format!("{} contains no official samples", path.display()).into());
    }

    for sample in &inventory.samples {
        if sample.category.trim().is_empty()
            || sample.name.trim().is_empty()
            || sample.source.trim().is_empty()
        {
            return Err(format!(
                "{} contains an official sample with an empty category, name, or source",
                path.display()
            )
            .into());
        }
    }
    for pair in inventory.samples.windows(2) {
        if pair[0] == pair[1] {
            return Err(format!(
                "{} duplicates official sample {}/{} ({})",
                path.display(),
                pair[0].category,
                pair[0].name,
                pair[0].source
            )
            .into());
        }
        if pair[0] > pair[1] {
            return Err(format!(
                "{} is not sorted at {}/{} ({})",
                path.display(),
                pair[1].category,
                pair[1].name,
                pair[1].source
            )
            .into());
        }
    }

    Ok(inventory.samples)
}

fn read_sample_parity_rows(root: &Path) -> Result<Vec<SampleParityRow>> {
    let matrix = root.join(SAMPLE_MATRIX_PATH);
    let source = fs::read_to_string(&matrix)?;
    let mut rows = Vec::new();
    let mut in_case_table = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == SAMPLE_CASE_TABLE_HEADER {
            in_case_table = true;
            continue;
        }
        if !in_case_table {
            continue;
        }
        if trimmed.starts_with("|---") {
            continue;
        }
        if !trimmed.starts_with('|') {
            if !rows.is_empty() {
                break;
            }
            continue;
        }

        let cells = split_markdown_table_row(trimmed);
        if cells.len() != 6 {
            return Err(format!(
                "{}:{} has {} cells, expected 6",
                matrix.display(),
                line_index + 1,
                cells.len()
            )
            .into());
        }
        let mode = SampleParityMode::try_from(cells[3].as_str()).map_err(|()| {
            format!(
                "{}:{} uses unknown parity mode `{}`",
                matrix.display(),
                line_index + 1,
                cells[3]
            )
        })?;
        rows.push(SampleParityRow {
            category: cells[0].clone(),
            name: cells[1].clone(),
            source: strip_code_ticks(&cells[2]),
            mode,
            target: cells[4].clone(),
            notes: cells[5].clone(),
            line_number: line_index + 1,
        });
    }

    if rows.is_empty() {
        Err(format!(
            "{} is missing the official case table header `{SAMPLE_CASE_TABLE_HEADER}`",
            matrix.display()
        )
        .into())
    } else {
        Ok(rows)
    }
}

fn split_markdown_table_row(row: &str) -> Vec<String> {
    row.trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn strip_code_ticks(value: &str) -> String {
    value.trim().trim_matches('`').to_string()
}

fn validate_sample_parity(
    root: &Path,
    official_samples: &[OfficialSample],
    parity_rows: &[SampleParityRow],
) -> Result<()> {
    let official_keys = official_samples.iter().collect::<BTreeSet<_>>();
    let mut row_keys = BTreeSet::new();
    for row in parity_rows {
        validate_sample_parity_row(root, row)?;
        let key = OfficialSample {
            category: row.category.clone(),
            name: row.name.clone(),
            source: row.source.clone(),
        };
        if !row_keys.insert(key) {
            return Err(format!(
                "{}:{} duplicates sample `{}` / `{}` at `{}`",
                SAMPLE_MATRIX_PATH, row.line_number, row.category, row.name, row.source
            )
            .into());
        }
    }

    let missing = official_samples
        .iter()
        .filter(|sample| !row_keys.contains(*sample))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "{} is missing {} official sample case(s): {}",
            SAMPLE_MATRIX_PATH,
            missing.len(),
            format_sample_key_list(&missing)
        )
        .into());
    }

    let extra = row_keys
        .iter()
        .filter(|sample| !official_keys.contains(sample))
        .collect::<Vec<_>>();
    if !extra.is_empty() {
        return Err(format!(
            "{} contains {} unknown sample case(s): {}",
            SAMPLE_MATRIX_PATH,
            extra.len(),
            format_sample_key_list(&extra)
        )
        .into());
    }

    Ok(())
}

fn validate_sample_parity_row(root: &Path, row: &SampleParityRow) -> Result<()> {
    if row.category.is_empty() || row.name.is_empty() || row.source.is_empty() {
        return Err(format!(
            "{}:{} has an empty category, sample, or source field",
            SAMPLE_MATRIX_PATH, row.line_number
        )
        .into());
    }
    if row.target.is_empty() || row.notes.is_empty() {
        return Err(format!(
            "{}:{} has an empty target or notes field",
            SAMPLE_MATRIX_PATH, row.line_number
        )
        .into());
    }
    if row.mode.requires_target() {
        validate_target_code_spans(root, row)?;
    }
    Ok(())
}

fn validate_target_code_spans(root: &Path, row: &SampleParityRow) -> Result<()> {
    let targets = extract_code_spans(&row.target);
    if targets.is_empty() {
        return Err(format!(
            "{}:{} target must contain at least one repo-relative code span",
            SAMPLE_MATRIX_PATH, row.line_number
        )
        .into());
    }

    if row.mode.requires_test_target() && !targets.iter().any(|target| is_test_target(target)) {
        return Err(format!(
            "{}:{} TestOnly target must include at least one nextest-backed `tests/` path",
            SAMPLE_MATRIX_PATH, row.line_number
        )
        .into());
    }

    for target in targets {
        let path = target_code_span_path(&target);
        if path.is_empty() {
            return Err(format!(
                "{}:{} target `{target}` has an empty path",
                SAMPLE_MATRIX_PATH, row.line_number
            )
            .into());
        }
        if !root.join(path).exists() {
            return Err(format!(
                "{}:{} target path does not exist: `{path}`",
                SAMPLE_MATRIX_PATH, row.line_number
            )
            .into());
        }
    }
    Ok(())
}

fn target_code_span_path(target: &str) -> &str {
    target
        .split_once('#')
        .map_or(target, |(path, _)| path)
        .trim()
}

fn is_test_target(target: &str) -> bool {
    let path = target_code_span_path(target);
    path.starts_with("tests/") || path.contains("/tests/")
}

fn extract_code_spans(value: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find('`') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('`') else {
            break;
        };
        spans.push(after_start[..end].to_string());
        rest = &after_start[end + 1..];
    }
    spans
}

fn format_sample_key_list(samples: &[&OfficialSample]) -> String {
    samples
        .iter()
        .take(10)
        .map(|sample| format!("{}/{} ({})", sample.category, sample.name, sample.source))
        .collect::<Vec<_>>()
        .join(", ")
}

fn generate_pages() -> Result<()> {
    let root = project_root();
    let pages_dir = root.join("docs").join("pages");
    validate_registry_catalog(&SCENE_CATALOG)?;
    generate_bevy_example_pages(&pages_dir, &SCENE_CATALOG)?;
    eprintln!(
        "Generated {} Bevy example pages under {}",
        SCENE_CATALOG.len(),
        pages_bevy_examples_dir().display()
    );
    Ok(())
}

fn validate_pages() -> Result<()> {
    let root = project_root();
    let pages_dir = root.join("docs").join("pages");
    let index = ensure_file(&pages_dir.join("index.html"), "Pages index")?;
    let testbed_index = ensure_file(
        &pages_dir.join("bevy-testbed").join("index.html"),
        "Bevy Web testbed page",
    )?;
    let loader = ensure_file(
        &pages_dir.join("bevy-testbed").join("loader.js"),
        "Bevy Web testbed loader",
    )?;

    validate_registry_catalog(&SCENE_CATALOG)?;
    validate_bevy_example_pages(&pages_dir, &SCENE_CATALOG)?;

    let html = fs::read_to_string(&index)?;
    validate_generated_page(
        &index,
        &html,
        &example_index_page(&SCENE_CATALOG, ExampleIndexLocation::Root),
    )?;
    validate_html_links(&index, &html)?;

    let testbed_html = fs::read_to_string(&testbed_index)?;
    validate_generated_page(&testbed_index, &testbed_html, &bevy_testbed_page())?;
    validate_html_links(&testbed_index, &testbed_html)?;
    validate_bevy_loader(&loader)?;

    eprintln!(
        "Validated Pages site: {} ({} Bevy examples)",
        pages_dir.display(),
        SCENE_CATALOG.len()
    );
    Ok(())
}

fn validate_bevy_loader(loader: &Path) -> Result<()> {
    let js = fs::read_to_string(loader)?;
    let expected = bevy_testbed_loader_js();
    if normalize_newlines(&js) != normalize_newlines(&expected) {
        return Err(format!(
            "{} is stale; run `cargo run -p xtask -- generate-pages`",
            loader.display()
        )
        .into());
    }
    Ok(())
}

fn generate_bevy_example_pages(pages_dir: &Path, samples: &[SceneCatalogEntry]) -> Result<()> {
    let examples_dir = pages_dir.join(BEVY_EXAMPLES_DIR);
    let testbed_dir = pages_dir.join("bevy-testbed");
    fs::create_dir_all(&examples_dir)?;
    fs::create_dir_all(&testbed_dir)?;
    fs::write(
        pages_dir.join("index.html"),
        example_index_page(samples, ExampleIndexLocation::Root),
    )?;
    fs::write(
        examples_dir.join("index.html"),
        example_index_page(samples, ExampleIndexLocation::ExamplesDirectory),
    )?;
    fs::write(testbed_dir.join("index.html"), bevy_testbed_page())?;
    fs::write(testbed_dir.join("loader.js"), bevy_testbed_loader_js())?;

    for sample in samples {
        let dir = examples_dir.join(sample.id);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("index.html"), example_page(sample))?;
    }

    Ok(())
}

fn validate_bevy_example_pages(pages_dir: &Path, samples: &[SceneCatalogEntry]) -> Result<()> {
    let examples_dir = ensure_file(
        &pages_dir.join(BEVY_EXAMPLES_DIR).join("index.html"),
        "Bevy examples index",
    )?;
    let examples_html = fs::read_to_string(&examples_dir)?;
    validate_generated_page(
        &examples_dir,
        &examples_html,
        &example_index_page(samples, ExampleIndexLocation::ExamplesDirectory),
    )?;
    validate_html_links(&examples_dir, &examples_html)?;

    for sample in samples {
        let page = ensure_file(
            &pages_dir
                .join(BEVY_EXAMPLES_DIR)
                .join(sample.id)
                .join("index.html"),
            &format!("Bevy example page `{}`", sample.id),
        )?;
        let html = fs::read_to_string(&page)?;
        validate_generated_page(&page, &html, &example_page(sample))?;
        validate_html_links(&page, &html)?;
    }

    Ok(())
}

fn validate_generated_page(path: &Path, actual: &str, expected: &str) -> Result<()> {
    if normalize_newlines(actual) == normalize_newlines(expected) {
        Ok(())
    } else {
        Err(format!(
            "{} is stale; run `cargo run -p xtask -- generate-pages`",
            path.display()
        )
        .into())
    }
}

fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n")
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ExampleIndexLocation {
    Root,
    ExamplesDirectory,
}

impl ExampleIndexLocation {
    fn home_href(self) -> &'static str {
        match self {
            Self::Root => "./",
            Self::ExamplesDirectory => "../",
        }
    }

    fn scene_href(self, id: &str) -> String {
        match self {
            Self::Root => format!("examples/{id}/"),
            Self::ExamplesDirectory => format!("{id}/"),
        }
    }
}

fn example_index_page(samples: &[SceneCatalogEntry], location: ExampleIndexLocation) -> String {
    let links = samples
        .iter()
        .map(|sample| {
            format!(
                "        <a class=\"card\" href=\"{href}\"><span>{category}</span><strong>{name}</strong><small>{description}</small><em>{upstream}</em></a>",
                href = location.scene_href(sample.id),
                category = escape_html(sample.category),
                name = escape_html(sample.name),
                description = escape_html(sample.description),
                upstream = source_summary(sample)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>boxddd Bevy Examples</title>
  <link rel="icon" href="data:,">
  <meta name="description" content="Direct Bevy Web examples for boxddd.">
  <style>{example_page_css}</style>
</head>
<body>
  <div class="directory">
    <header class="topbar">
      <a href="{home_href}">boxddd Examples</a>
      <nav>
        <a href="https://github.com/Latias94/boxddd">GitHub</a>
        <a href="https://docs.rs/boxddd">Docs.rs</a>
      </nav>
    </header>
    <main class="directory-main">
      <p class="eyebrow">Bevy Web examples</p>
      <h1>Run a Box3D scene</h1>
      <p class="lead">Each entry opens a dedicated Bevy + egui WASM page backed by the same Box3D provider runtime.</p>
      <section class="card-grid">
{links}
      </section>
    </main>
  </div>
</body>
</html>
"#,
        example_page_css = example_page_css(),
        home_href = location.home_href(),
        links = links
    )
}

fn bevy_testbed_page() -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>boxddd Bevy Testbed</title>
  <link rel="icon" href="data:,">
  <meta name="description" content="Bevy + egui WASM testbed for boxddd.">
  <style>{example_page_css}</style>
</head>
<body>
  <div class="shell">
    <header class="topbar">
      <div>
        <a href="../">boxddd Examples</a>
        <h1>Bevy Testbed</h1>
        <p><span>All scenes</span> Switch scenes from the egui panel.</p>
      </div>
      <nav>
        <a href="../examples/">All Bevy examples</a>
        <a href="https://github.com/Latias94/boxddd/tree/main/bevy_boxddd/examples/testbed_3d">Source</a>
      </nav>
    </header>
    <main id="bevy-app" data-scene-id="" data-scene-name="Bevy Testbed" data-scene-category="All scenes">
      <canvas id="bevy-canvas" tabindex="0"></canvas>
      <div id="bevy-status" role="status" aria-live="polite">
        <strong>Loading Bevy Testbed</strong>
        <span>Preparing the shared Box3D provider and the Rust Bevy wasm module.</span>
      </div>
    </main>
  </div>
  <script type="module" src="loader.js"></script>
</body>
</html>
"#,
        example_page_css = example_page_css()
    )
}

fn example_page(sample: &SceneCatalogEntry) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{name} - boxddd Bevy Example</title>
  <link rel="icon" href="data:,">
  <meta name="description" content="{description}">
  <style>{example_page_css}</style>
</head>
<body>
  <div class="shell">
    <header class="topbar">
      <div>
        <a href="../../">boxddd Examples</a>
        <h1>{name}</h1>
        <p><span>{category}</span>{description}</p>
        {upstream}
      </div>
      <nav>
        <a href="../">All Bevy examples</a>
        <a href="https://github.com/Latias94/boxddd/tree/main/bevy_boxddd/examples/testbed_3d">Source</a>
      </nav>
    </header>
    <main id="bevy-app" data-scene-id="{id}" data-scene-name="{name}" data-scene-category="{category}">
      <canvas id="bevy-canvas" tabindex="0"></canvas>
      <div id="bevy-status" role="status" aria-live="polite">
        <strong>Loading {name}</strong>
        <span>Preparing the shared Box3D provider and the Rust Bevy wasm module.</span>
      </div>
    </main>
  </div>
  <script type="module" src="../../bevy-testbed/loader.js"></script>
</body>
</html>
"#,
        id = sample.id,
        name = escape_html(sample.name),
        category = escape_html(sample.category),
        description = escape_html(sample.description),
        upstream = source_list_html(sample),
        example_page_css = example_page_css()
    )
}

fn bevy_testbed_loader_js() -> String {
    r##"const statusPanel = document.querySelector("#bevy-status");
const appRoot = document.querySelector("#bevy-app");
const sceneId = appRoot?.dataset.sceneId || "";
const sceneName = appRoot?.dataset.sceneName || "Bevy testbed";
const isExamplePage = Boolean(sceneId);

function setStatus(state, title, detail, progress) {
  statusPanel.dataset.state = state;
  statusPanel.replaceChildren();

  const titleNode = document.createElement("strong");
  titleNode.textContent = title;
  const detailNode = document.createElement("span");
  detailNode.textContent = detail;
  statusPanel.append(titleNode, detailNode);

  if (progress) {
    const progressNode = document.createElement("progress");
    progressNode.value = progress.loaded;
    if (progress.total) {
      progressNode.max = progress.total;
    } else {
      progressNode.removeAttribute("value");
    }

    const progressText = document.createElement("small");
    progressText.textContent = progressTextFor(progress.loaded, progress.total);
    statusPanel.append(progressNode, progressText);
  }
}

function generatedUrl(path) {
  return new URL(path, import.meta.url);
}

function progressTextFor(loaded, total) {
  if (total) {
    const percent = Math.min(100, Math.round((loaded / total) * 100));
    return `${formatBytes(loaded)} / ${formatBytes(total)} (${percent}%)`;
  }
  return `${formatBytes(loaded)} downloaded`;
}

function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${value} ${units[unit]}` : `${value.toFixed(2)} ${units[unit]}`;
}

async function fetchArrayBufferWithProgress(url, label) {
  setStatus("loading", `Downloading ${label}`, "Starting download.", { loaded: 0, total: 0 });
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${label} download failed with HTTP ${response.status}`);
  }

  const total = Number(response.headers.get("Content-Length")) || 0;
  if (!response.body) {
    const buffer = await response.arrayBuffer();
    setStatus("loading", `Downloading ${label}`, "Download complete.", {
      loaded: buffer.byteLength,
      total: total || buffer.byteLength,
    });
    return buffer;
  }

  const reader = response.body.getReader();
  const chunks = [];
  let loaded = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    chunks.push(value);
    loaded += value.byteLength;
    setStatus("loading", `Downloading ${label}`, "Downloading runtime asset.", { loaded, total });
  }

  const bytes = new Uint8Array(loaded);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  setStatus("loading", `Downloading ${label}`, "Download complete.", { loaded, total: total || loaded });
  return bytes.buffer;
}

async function main() {
  const providerGenerated = new URL("../wasm/generated/", import.meta.url);
  const providerWasmUrl = new URL("__BOXDDD_PROVIDER_ASSET__.wasm", providerGenerated);
  const bevyWasmUrl = generatedUrl("generated/bevy_boxddd_testbed_bg.wasm");

  setStatus("loading", "Loading JavaScript modules", `Preparing the browser runtime for ${sceneName}.`);
  const [
    { default: createProvider },
    { default: initBevyTestbed },
    { setBox3dProvider, releaseBoxdddConsumer },
  ] =
    await Promise.all([
      import(new URL("__BOXDDD_PROVIDER_ASSET__.js", providerGenerated).href),
      import(generatedUrl("generated/bevy_boxddd_testbed.js").href),
      import(generatedUrl("generated/box3d-provider-shim.js").href),
    ]);
  const memory = new WebAssembly.Memory({ initial: 4096, maximum: 8192 });

  const providerWasm = await fetchArrayBufferWithProgress(providerWasmUrl, "Box3D provider wasm");
  setStatus("loading", "Starting Box3D provider", `Instantiating the shared Box3D C provider for ${sceneName}.`);
  const provider = await createProvider({
    wasmMemory: memory,
    wasmBinary: providerWasm,
    locateFile: (path) => new URL(path, providerGenerated).href,
    print: (text) => console.log(`[__BOXDDD_PROVIDER_MODULE__] ${text}`),
    printErr: (text) => console.warn(`[__BOXDDD_PROVIDER_MODULE__] ${text}`),
  });

  if (provider.wasmMemory && provider.wasmMemory !== memory) {
    throw new Error("Box3D provider did not use the shared WebAssembly.Memory");
  }
  const readProviderRevision =
    provider._boxddd_provider_abi_revision || provider.boxddd_provider_abi_revision;
  if (typeof readProviderRevision !== "function") {
    throw new Error("Box3D provider is missing its ABI revision sentinel");
  }
  const providerRevision = readProviderRevision();
  if (providerRevision !== __BOXDDD_PROVIDER_BRIDGE_REVISION__) {
    throw new Error(
      `Box3D provider ABI revision ${providerRevision} does not match expected revision __BOXDDD_PROVIDER_BRIDGE_REVISION__`,
    );
  }

  setBox3dProvider(provider);
  const bevyWasm = await fetchArrayBufferWithProgress(bevyWasmUrl, `${sceneName} Bevy wasm`);
  setStatus("loading", `Starting ${sceneName}`, "Instantiating the Rust Bevy + egui wasm module.");

  const bevyExports = await initBevyTestbed({
    module_or_path: bevyWasm,
    memory,
  });
  window.addEventListener("pagehide", (event) => {
    if (!event.persisted) {
      releaseBoxdddConsumer(bevyExports);
    }
  });

  window.BOXDDD_BEVY_TESTBED_READY = true;
  window.BOXDDD_BEVY_EXAMPLE_READY = true;
  window.BOXDDD_BEVY_SCENE_ID = sceneId;
  setStatus(
    "running",
    `${sceneName} running`,
    isExamplePage
      ? "This dedicated example page is running the selected Box3D scene in Bevy."
      : "The scene browser, egui controls, picking, and Box3D simulation are running in this canvas.",
  );
}

main().catch((error) => {
  console.error(error);
  const message = error instanceof Error ? error.message : String(error);
  setStatus("error", `${sceneName} failed`, message);
});
"##
    .replace("__BOXDDD_PROVIDER_ASSET__", PROVIDER_ASSET_BASENAME)
    .replace("__BOXDDD_PROVIDER_MODULE__", PROVIDER_MODULE)
    .replace(
        "__BOXDDD_PROVIDER_BRIDGE_REVISION__",
        &PROVIDER_BRIDGE_REVISION.to_string(),
    )
}

fn source_summary(sample: &SceneCatalogEntry) -> String {
    if let Some(lesson) = sample.showcase_lesson {
        escape_html(&format!("boxddd showcase: {lesson}"))
    } else {
        upstream_summary(sample.upstream)
    }
}

fn upstream_summary(upstream: &[scene_catalog::UpstreamSampleRef]) -> String {
    let mut labels = upstream
        .iter()
        .take(3)
        .map(|sample| format!("{} / {}", sample.category, sample.name))
        .collect::<Vec<_>>();
    if upstream.len() > labels.len() {
        labels.push(format!("+{} more", upstream.len() - labels.len()));
    }
    escape_html(&labels.join(", "))
}

fn source_list_html(sample: &SceneCatalogEntry) -> String {
    let mut items = String::new();
    if let Some(lesson) = sample.showcase_lesson {
        items.push_str(&format!(
            "<span>boxddd showcase · {lesson}</span>",
            lesson = escape_html(lesson)
        ));
    }
    for upstream in sample.upstream {
        write!(
            items,
            "<span>{category} / {name} · {mode}</span>",
            category = escape_html(upstream.category),
            name = escape_html(upstream.name),
            mode = parity_mode_label(upstream.mode)
        )
        .expect("writing to String cannot fail");
    }
    format!(r#"<div class="upstream-list">{items}</div>"#)
}

fn parity_mode_label(mode: ParityMode) -> &'static str {
    match mode {
        ParityMode::FaithfulPort => "faithful port",
        ParityMode::TeachingAdaptation => "teaching adaptation",
    }
}

fn example_page_css() -> &'static str {
    r#"
:root {
  color-scheme: dark;
  --background: #09090b;
  --foreground: #fafafa;
  --card: #0f0f12;
  --muted: #a1a1aa;
  --border: #27272a;
  --accent: #84cc16;
  --danger: #f87171;
}
* { box-sizing: border-box; }
html, body { width: 100%; height: 100%; margin: 0; background: var(--background); color: var(--foreground); font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
a { color: var(--foreground); text-decoration: none; }
a:hover { text-decoration: underline; text-underline-offset: 4px; }
.shell { display: grid; grid-template-rows: auto minmax(0, 1fr); width: 100%; height: 100%; }
.topbar { display: flex; flex-wrap: wrap; gap: 14px; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border); background: rgba(9, 9, 11, 0.94); padding: 14px 18px; }
.topbar h1 { margin: 4px 0 0; font-size: 20px; line-height: 1.2; letter-spacing: 0; }
.topbar p { display: flex; flex-wrap: wrap; gap: 8px; margin: 5px 0 0; color: var(--muted); font-size: 13px; }
.topbar p span, .eyebrow { color: var(--accent); font-weight: 700; text-transform: uppercase; }
.topbar nav { display: flex; flex-wrap: wrap; gap: 12px; color: var(--muted); font-size: 14px; }
#bevy-app { position: relative; min-width: 0; min-height: 0; background: #020617; }
#bevy-canvas { display: block; width: 100%; height: 100%; outline: none; touch-action: none; }
#bevy-status { position: absolute; left: 18px; bottom: 18px; max-width: min(560px, calc(100% - 36px)); border: 1px solid var(--border); border-radius: 8px; background: rgba(15, 15, 18, 0.94); padding: 12px 14px; color: var(--muted); font-size: 14px; line-height: 1.45; }
#bevy-status strong { display: block; margin-bottom: 4px; color: var(--foreground); font-size: 15px; }
#bevy-status progress { display: block; width: min(360px, 100%); height: 8px; margin-top: 10px; accent-color: var(--accent); }
#bevy-status small { display: block; margin-top: 6px; color: #d4d4d8; font-size: 12px; }
#bevy-status[data-state="error"] strong { color: var(--danger); }
#bevy-status[data-state="running"] { opacity: 0; pointer-events: none; transition: opacity 180ms ease; }
.directory { min-height: 100%; }
.directory-main { width: min(1180px, calc(100% - 32px)); margin: 0 auto; padding: 54px 0; }
.directory-main h1 { margin: 0; font-size: clamp(34px, 6vw, 58px); line-height: 1; letter-spacing: 0; }
.lead { max-width: 720px; color: var(--muted); font-size: 17px; }
.card-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 12px; margin-top: 28px; }
.card { display: grid; min-height: 150px; gap: 8px; border: 1px solid var(--border); border-radius: 8px; background: var(--card); padding: 16px; }
.card:hover { border-color: #52525b; text-decoration: none; }
.card span { color: var(--accent); font-size: 12px; font-weight: 700; text-transform: uppercase; }
.card strong { font-size: 18px; }
.card small { color: var(--muted); font-size: 13px; line-height: 1.5; }
.card em { color: #d4d4d8; font-size: 12px; font-style: normal; line-height: 1.45; }
.upstream-list { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
.upstream-list span { border: 1px solid var(--border); border-radius: 999px; background: rgba(39, 39, 42, 0.7); padding: 4px 7px; color: #d4d4d8; font-size: 12px; line-height: 1.2; text-transform: none; }
"#
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn ensure_file(path: &Path, label: &str) -> Result<PathBuf> {
    if path.is_file() {
        Ok(path.to_path_buf())
    } else {
        Err(format!("{label} is missing: {}", path.display()).into())
    }
}

fn validate_registry_catalog(samples: &[SceneCatalogEntry]) -> Result<()> {
    if samples.is_empty() {
        return Err("testbed registry must contain at least one entry".into());
    }

    let mut seen = BTreeSet::new();
    for sample in samples {
        validate_registry_field(sample.id, "id", sample.id)?;
        validate_registry_field(sample.id, "category", sample.category)?;
        validate_registry_field(sample.id, "name", sample.name)?;
        validate_registry_field(sample.id, "description", sample.description)?;
        let has_upstream = !sample.upstream.is_empty();
        let has_showcase_lesson = sample.showcase_lesson.is_some();
        if !has_upstream && !has_showcase_lesson {
            return Err(format!(
                "testbed registry sample `{}` must include upstream sample references or a showcase lesson",
                sample.id
            )
            .into());
        }
        if has_upstream && has_showcase_lesson {
            return Err(format!(
                "testbed registry sample `{}` must not include both upstream sample references and a showcase lesson",
                sample.id
            )
            .into());
        }
        if let Some(lesson) = sample.showcase_lesson {
            validate_registry_field(sample.id, "showcase_lesson", lesson)?;
        }

        if !is_slug(sample.id) {
            return Err(format!(
                "testbed registry id `{}` must be a lowercase ASCII slug",
                sample.id
            )
            .into());
        }
        if !seen.insert(sample.id) {
            return Err(format!("duplicate testbed registry id `{}`", sample.id).into());
        }

        let mut upstream_seen = BTreeSet::new();
        for upstream in sample.upstream {
            validate_registry_field(sample.id, "upstream.category", upstream.category)?;
            validate_registry_field(sample.id, "upstream.name", upstream.name)?;
            if !upstream_seen.insert((upstream.category, upstream.name)) {
                return Err(format!(
                    "testbed registry sample `{}` duplicates upstream ref `{}` / `{}`",
                    sample.id, upstream.category, upstream.name
                )
                .into());
            }
        }
    }

    Ok(())
}

fn validate_registry_field(sample_id: &str, field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(format!(
            "testbed registry sample `{}` has an empty `{field}` field",
            sample_id
        )
        .into())
    } else {
        Ok(())
    }
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

impl BuildProfile {
    fn from_env() -> Self {
        match env::var("PROFILE").as_deref() {
            Ok("release") => Self::Release,
            _ => Self::Debug,
        }
    }

    fn for_pages() -> Result<Self> {
        match env::var(PAGES_WASM_PROFILE_ENV) {
            Ok(value) => Self::parse(&value).ok_or_else(|| {
                format!(
                    "invalid {PAGES_WASM_PROFILE_ENV} value `{value}`; expected debug, release, or wasm-release"
                )
                .into()
            }),
            Err(_) => Ok(Self::WasmRelease),
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "debug" | "Debug" | "DEBUG" => Some(Self::Debug),
            "release" | "Release" | "RELEASE" => Some(Self::Release),
            "wasm-release" | "WASM-RELEASE" | "wasm_release" | "WASM_RELEASE" => {
                Some(Self::WasmRelease)
            }
            _ => None,
        }
    }

    fn cargo_args(self) -> &'static [&'static str] {
        match self {
            Self::Debug => &[],
            Self::Release => &["--release"],
            Self::WasmRelease => &["--profile", "wasm-release"],
        }
    }

    fn target_dir(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
            Self::WasmRelease => "wasm-release",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
            Self::WasmRelease => "wasm-release",
        }
    }
}

fn validate_html_links(html_file: &Path, html: &str) -> Result<()> {
    let pages_dir = project_root().join("docs").join("pages");
    let pages_root = fs::canonicalize(&pages_dir)?;
    let base_dir = html_file
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", html_file.display()))?;
    validate_attr_links(html_file, base_dir, &pages_root, html, "href")?;
    validate_attr_links(html_file, base_dir, &pages_root, html, "src")?;
    Ok(())
}

fn validate_attr_links(
    html_file: &Path,
    base_dir: &Path,
    pages_root: &Path,
    html: &str,
    attr: &str,
) -> Result<()> {
    let needle = format!("{attr}=\"");
    let mut remainder = html;
    while let Some(index) = remainder.find(&needle) {
        let after = &remainder[index + needle.len()..];
        let end = after
            .find('"')
            .ok_or_else(|| format!("unterminated `{attr}` attribute in {}", html_file.display()))?;
        validate_local_link(html_file, base_dir, pages_root, &after[..end])?;
        remainder = &after[end + 1..];
    }
    Ok(())
}

fn validate_local_link(
    html_file: &Path,
    base_dir: &Path,
    pages_root: &Path,
    value: &str,
) -> Result<()> {
    if is_external_or_fragment(value) {
        return Ok(());
    }

    let local = strip_url_suffix(value);
    if local.is_empty() {
        return Ok(());
    }

    let target = base_dir.join(local);
    if !target.exists() {
        return Err(format!(
            "{} links missing local asset `{value}`",
            html_file.display()
        )
        .into());
    }

    let canonical = fs::canonicalize(&target)?;
    if !canonical.starts_with(pages_root) {
        return Err(format!("{} link escapes docs/pages: `{value}`", html_file.display()).into());
    }

    Ok(())
}

fn is_external_or_fragment(value: &str) -> bool {
    value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("mailto:")
        || value.starts_with("data:")
        || value.starts_with('#')
        || value.starts_with("javascript:")
}

fn strip_url_suffix(value: &str) -> &str {
    let query = value.find('?').unwrap_or(value.len());
    let fragment = value.find('#').unwrap_or(value.len());
    &value[..query.min(fragment)]
}

fn build_provider_smoke_app() -> Result<PathBuf> {
    build_provider_smoke_app_for(BuildProfile::from_env())
}

fn build_provider_smoke_app_for(profile: BuildProfile) -> Result<PathBuf> {
    let root = project_root();
    let mut command = Command::new("cargo");
    command
        .arg("rustc")
        .arg("-p")
        .arg(SMOKE_PACKAGE)
        .arg("--lib")
        .arg("--target")
        .arg(TARGET)
        .args(profile.cargo_args())
        .env("BOXDDD_SYS_WASM_MODE", "provider");
    add_wasm_app_link_args(
        &mut command,
        &[
            PROVIDER_SMOKE_EXPORTS,
            DEBUG_BRIDGE_EXPORTS,
            QUERY_BRIDGE_EXPORTS,
        ],
    );
    run_command(&mut command, "build provider-smoke Rust wasm")?;

    let wasm = root
        .join("target")
        .join(TARGET)
        .join(profile.target_dir())
        .join(SMOKE_WASM);
    if !wasm.exists() {
        return Err(format!("provider-smoke wasm artifact not found: {}", wasm.display()).into());
    }

    let out_dir = provider_smoke_dir();
    fs::create_dir_all(&out_dir)?;
    fs::copy(&wasm, out_dir.join(SMOKE_WASM))?;
    Ok(wasm)
}

fn add_wasm_app_link_args(command: &mut Command, export_groups: &[&[&str]]) {
    command.arg("--").arg("-C").arg("link-arg=--import-memory");
    for export in export_groups.iter().flat_map(|exports| exports.iter()) {
        command.arg("-C").arg(format!("link-arg=--export={export}"));
    }
}

fn collect_provider_imports(wasm: &Path) -> Result<Vec<String>> {
    ensure_tool(
        "node",
        "--version",
        "Node.js is required for provider smoke",
    )?;
    let script = r#"
const fs = require('node:fs');
const wasmPath = process.argv[1];
const providerModule = process.argv[2];
const module = new WebAssembly.Module(fs.readFileSync(wasmPath));
const names = WebAssembly.Module.imports(module)
  .filter((i) => i.kind === 'function' && i.module === providerModule)
  .map((i) => i.name)
  .sort();
for (const name of names) console.log(name);
"#;
    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .arg(wasm)
        .arg(PROVIDER_MODULE)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "failed to inspect wasm imports with node: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let imports = String::from_utf8(output.stdout)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if imports.is_empty() {
        return Err(format!(
            "{} does not import any functions from {PROVIDER_MODULE}",
            wasm.display()
        )
        .into());
    }
    Ok(imports)
}

fn write_exports_json(out_dir: &Path, imports: &[String]) -> Result<PathBuf> {
    fs::create_dir_all(out_dir)?;
    let exported = imports
        .iter()
        .map(|name| format!("\"_{name}\""))
        .chain(std::iter::once(
            "\"_boxddd_provider_abi_revision\"".to_string(),
        ))
        .collect::<BTreeSet<_>>();
    let path = out_dir.join("box3d-provider-exports.json");
    fs::write(
        &path,
        format!("[{}]", exported.into_iter().collect::<Vec<_>>().join(",")),
    )?;
    Ok(path)
}

fn build_bevy_web_app() -> Result<BevyWebArtifacts> {
    ensure_tool(
        "wasm-bindgen",
        "--version",
        "wasm-bindgen-cli is required for Bevy Web examples",
    )?;
    let profile = BuildProfile::for_pages()?;

    let root = project_root();
    let out_dir = root.join("target").join("boxddd-bevy-testbed-web");
    replace_dir_under(&out_dir, &root.join("target"))?;

    let mut command = Command::new("cargo");
    command
        .arg("rustc")
        .arg("-p")
        .arg("bevy_boxddd")
        .arg("--features")
        .arg("debug-gizmos physics-picking")
        .arg("--example")
        .arg(BEVY_WEB_EXAMPLE)
        .arg("--target")
        .arg(TARGET)
        .args(profile.cargo_args())
        .env("BOXDDD_SYS_WASM_MODE", "provider");
    add_wasm_app_link_args(&mut command, &[DEBUG_BRIDGE_EXPORTS, QUERY_BRIDGE_EXPORTS]);
    run_command(
        &mut command,
        &format!("build Bevy testbed wasm ({})", profile.label()),
    )?;

    let wasm = root
        .join("target")
        .join(TARGET)
        .join(profile.target_dir())
        .join("examples")
        .join(format!("{BEVY_WEB_EXAMPLE}.wasm"));
    ensure_file(&wasm, "Bevy testbed wasm")?;

    let mut bindgen = Command::new("wasm-bindgen");
    bindgen
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--out-name")
        .arg(BEVY_WEB_OUT_NAME)
        .arg(&wasm);
    run_command(&mut bindgen, "run wasm-bindgen for Bevy testbed")?;

    patch_bevy_bindgen_imports(&out_dir.join(BEVY_WEB_JS))?;
    let bevy_wasm = out_dir.join(BEVY_WEB_WASM);
    optimize_wasm_if_available(&bevy_wasm, "Bevy testbed wasm")?;
    let imports = collect_provider_imports(&bevy_wasm)?;
    write_browser_provider_shim(&out_dir, &imports)?;

    Ok(BevyWebArtifacts { out_dir, imports })
}

fn patch_bevy_bindgen_imports(js: &Path) -> Result<()> {
    let source = fs::read_to_string(js)?;
    let patched_imports = source.replace(
        &format!("from \"{PROVIDER_MODULE}\""),
        &format!("from \"./{BEVY_PROVIDER_SHIM}\""),
    );
    if patched_imports == source {
        return Err(format!(
            "wasm-bindgen output does not import {PROVIDER_MODULE}: {}",
            js.display()
        )
        .into());
    }
    let patched = patched_imports.replace(
        "    wasm = instance.exports;\n",
        "    wasm = instance.exports;\n    if (typeof import1.acquireBoxdddConsumer === \"function\") {\n        import1.acquireBoxdddConsumer(wasm);\n    }\n",
    );
    if patched == patched_imports {
        return Err(format!(
            "wasm-bindgen output does not assign instance exports: {}",
            js.display()
        )
        .into());
    }
    let release_patched = patched.replace(
        "    wasm.__wbindgen_start();\n    return wasm;\n",
        "    try {\n        wasm.__wbindgen_start();\n    } catch (error) {\n        if (typeof import1.releaseBoxdddConsumer === \"function\") {\n            import1.releaseBoxdddConsumer(wasm);\n        }\n        throw error;\n    }\n    return wasm;\n",
    );
    if release_patched == patched {
        return Err(format!(
            "wasm-bindgen output does not start the wasm module: {}",
            js.display()
        )
        .into());
    }
    let decode_patched = release_patched.replace(
        "cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len))",
        "cachedTextDecoder.decode(getUint8ArrayMemory0().slice(ptr, ptr + len))",
    );
    if decode_patched == release_patched {
        return Err(format!(
            "wasm-bindgen output does not decode strings from wasm memory: {}",
            js.display()
        )
        .into());
    }
    fs::write(js, decode_patched)?;
    Ok(())
}

fn write_browser_provider_shim(out_dir: &Path, imports: &[String]) -> Result<PathBuf> {
    let exports = imports
        .iter()
        .map(|name| {
            format!("export function {name}(...args) {{ return callProvider(\"{name}\", args); }}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let shim = format!(
        r#"let provider;
let activeConsumer;

export function setBox3dProvider(nextProvider) {{
  if (provider && provider !== nextProvider && activeConsumer) {{
    throw new Error("cannot replace a Box3D provider while a Rust consumer is active");
  }}
  provider = nextProvider;
}}

function requireProvider() {{
  if (!provider) {{
    throw new Error("Box3D provider is not initialized");
  }}
  return provider;
}}

export function acquireBoxdddConsumer(exports) {{
  if (!exports) {{
    throw new Error("Box3D Rust consumer exports are required");
  }}
  const currentProvider = requireProvider();
  if (activeConsumer) {{
    throw new Error("the Box3D provider already has an active Rust consumer");
  }}
  currentProvider.boxdddAppExports = exports;
  activeConsumer = exports;
  let released = false;
  return () => {{
    if (released) {{
      throw new Error("the Box3D Rust consumer was already released");
    }}
    releaseBoxdddConsumer(exports);
    released = true;
  }};
}}

export function releaseBoxdddConsumer(exports) {{
  if (activeConsumer !== exports) {{
    throw new Error("cannot release a Rust consumer that is no longer active");
  }}
  const currentProvider = requireProvider();
  currentProvider.boxdddAppExports = undefined;
  activeConsumer = undefined;
}}

function resolveProviderExport(name) {{
  const currentProvider = requireProvider();
  const exported = currentProvider[`_${{name}}`] || currentProvider[name];
  if (typeof exported !== "function") {{
    throw new Error(`Box3D provider is missing export ${{name}}`);
  }}
  return exported;
}}

function callProvider(name, args) {{
  return resolveProviderExport(name)(...args);
}}

{exports}
"#
    );
    let path = out_dir.join(BEVY_PROVIDER_SHIM);
    fs::write(&path, shim)?;
    Ok(path)
}

fn copy_bevy_web_artifacts(artifacts: &BevyWebArtifacts) -> Result<()> {
    let generated = pages_bevy_generated_dir();
    replace_dir_under(&generated, &project_root().join("docs").join("pages"))?;

    for file in [BEVY_WEB_JS, BEVY_WEB_WASM, BEVY_PROVIDER_SHIM] {
        fs::copy(artifacts.out_dir.join(file), generated.join(file))?;
    }

    Ok(())
}

fn replace_dir_under(dir: &Path, allowed_root: &Path) -> Result<()> {
    fs::create_dir_all(allowed_root)?;
    if dir.exists() {
        let canonical_dir = fs::canonicalize(dir)?;
        let canonical_root = fs::canonicalize(allowed_root)?;
        if !canonical_dir.starts_with(&canonical_root) {
            return Err(format!(
                "refusing to remove directory outside {}: {}",
                canonical_root.display(),
                canonical_dir.display()
            )
            .into());
        }
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)?;
    Ok(())
}

fn run_provider_smoke() -> Result<()> {
    let app_wasm = build_provider_smoke_app()?;
    let imports = collect_provider_imports(&app_wasm)?;
    let out_dir = provider_smoke_dir();
    let exports = write_exports_json(&out_dir, &imports)?;
    let provider = build_box3d_provider(&out_dir, &exports)?;
    let app_copy = out_dir.join(SMOKE_WASM);
    write_browser_provider_shim(&out_dir, &imports)?;
    let contract = write_provider_smoke_contract(&out_dir, &provider, &app_copy, &imports)?;

    let runner = project_root()
        .join("examples-wasm")
        .join("provider-smoke")
        .join("run-provider-smoke.mjs");
    let mut command = Command::new("node");
    command.arg(runner).arg(contract);
    run_command(&mut command, "run provider shared-memory smoke")?;
    Ok(())
}

fn build_pages_wasm() -> Result<()> {
    generate_pages()?;
    let bevy_artifacts = build_bevy_web_app()?;
    let out_dir = provider_smoke_dir();
    let exports = write_exports_json(&out_dir, &bevy_artifacts.imports)?;
    let provider = build_box3d_provider(&out_dir, &exports)?;
    let provider_wasm = provider.with_extension("wasm");
    ensure_file(&provider, "Box3D provider module")?;
    ensure_file(&provider_wasm, "Box3D provider wasm")?;
    optimize_wasm_if_available(&provider_wasm, "Box3D provider wasm")?;

    let generated = pages_wasm_generated_dir();
    replace_dir_under(&generated, &project_root().join("docs").join("pages"))?;

    fs::copy(
        &provider,
        generated.join(format!("{PROVIDER_ASSET_BASENAME}.js")),
    )?;
    fs::copy(
        &provider_wasm,
        generated.join(format!("{PROVIDER_ASSET_BASENAME}.wasm")),
    )?;
    copy_bevy_web_artifacts(&bevy_artifacts)?;

    eprintln!(
        "Pages WASM assets ready: {} and {} ({} Bevy imports, {} provider exports)",
        generated.display(),
        pages_bevy_generated_dir().display(),
        bevy_artifacts.imports.len(),
        bevy_artifacts.imports.len()
    );
    Ok(())
}

fn build_box3d_provider(out_dir: &Path, exports_json: &Path) -> Result<PathBuf> {
    let emcc = find_emcc()?;
    let root = project_root();
    let box3d_root = root.join("boxddd-sys").join("third-party").join("box3d");
    let include_dir = box3d_root.join("include");
    let src_dir = box3d_root.join("src");
    let provider_helper = root
        .join("boxddd-sys")
        .join("provider")
        .join("debug_callbacks.c");
    let provider = out_dir.join(format!("{PROVIDER_ASSET_BASENAME}.js"));
    let c_files = BOX3D_C_SOURCES
        .iter()
        .map(|relative| {
            let file = box3d_root.join(relative);
            if file.is_file() {
                Ok(file)
            } else {
                Err(format!(
                    "manifest-declared Box3D source is missing: {}",
                    file.display()
                )
                .into())
            }
        })
        .collect::<Result<Vec<_>>>()?;

    let mut command = Command::new(emcc);
    command
        .arg("-std=c17")
        .arg("-O2")
        .arg("-s")
        .arg("MODULARIZE=1")
        .arg("-s")
        .arg("EXPORT_ES6=1")
        .arg("-s")
        .arg("ENVIRONMENT=node,web")
        .arg("-s")
        .arg("INCOMING_MODULE_JS_API=['wasmMemory','wasmBinary','locateFile','print','printErr']")
        .arg("-s")
        .arg("GLOBAL_BASE=67108864")
        .arg("-s")
        .arg("IMPORTED_MEMORY=1")
        .arg("-s")
        .arg("ALLOW_MEMORY_GROWTH=1")
        .arg("-s")
        .arg("INITIAL_MEMORY=134217728")
        .arg("-s")
        .arg("MAXIMUM_MEMORY=536870912")
        .arg("-s")
        .arg("FILESYSTEM=0")
        .arg("-s")
        .arg("NO_EXIT_RUNTIME=1")
        .arg("-s")
        .arg("MALLOC=emmalloc")
        .arg("-s")
        .arg("ASSERTIONS=1")
        .arg("-s")
        .arg("STACK_SIZE=1048576")
        .arg("-s")
        .arg("ERROR_ON_UNDEFINED_SYMBOLS=1")
        .arg("-s")
        .arg(format!(
            "EXPORTED_FUNCTIONS=@{}",
            exports_json.to_string_lossy().replace('\\', "/")
        ))
        .arg("-DBOX3D_DISABLE_SIMD")
        .arg("-DBOX3D_WASM_SINGLE_THREADED")
        .arg(format!(
            "-DBOXDDD_PROVIDER_ABI_REVISION={PROVIDER_BRIDGE_REVISION}"
        ))
        .arg("-I")
        .arg(&include_dir)
        .arg("-I")
        .arg(&src_dir);
    for file in c_files {
        command.arg(file);
    }
    command.arg(provider_helper);
    command.arg("-o").arg(&provider);
    run_command(&mut command, "build Box3D provider wasm")?;
    Ok(provider)
}

fn write_provider_smoke_contract(
    out_dir: &Path,
    provider: &Path,
    app_wasm: &Path,
    imports: &[String],
) -> Result<PathBuf> {
    let provider_file = provider
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid provider file name")?;
    let app_wasm_file = app_wasm
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid app wasm file name")?;
    let contract = ProviderSmokeContract {
        provider_module: PROVIDER_MODULE,
        provider_bridge_revision: PROVIDER_BRIDGE_REVISION,
        provider_file,
        app_wasm_file,
        shim_file: BEVY_PROVIDER_SHIM,
        provider_imports: imports,
        required_app_exports: PROVIDER_SMOKE_EXPORTS
            .iter()
            .chain(DEBUG_BRIDGE_EXPORTS)
            .chain(QUERY_BRIDGE_EXPORTS)
            .copied()
            .collect(),
    };
    let path = out_dir.join("provider-smoke-contract.json");
    fs::write(&path, serde_json::to_vec_pretty(&contract)?)?;
    fs::write(out_dir.join("package.json"), r#"{"type":"module"}"#)?;
    Ok(path)
}

fn find_emcc() -> Result<PathBuf> {
    if let Some(path) = runnable_tool("emcc", "--version") {
        return Ok(path);
    }

    if let Ok(root) = env::var("EMSDK") {
        let emscripten = PathBuf::from(root).join("upstream").join("emscripten");
        let emcc = if cfg!(windows) {
            emscripten.join("emcc.bat")
        } else {
            emscripten.join("emcc")
        };
        if emcc.exists() {
            return Ok(emcc);
        }
    }

    Err(
        "failed to locate emcc; install emsdk, run emsdk_env, or set EMSDK to the emsdk root"
            .into(),
    )
}

fn optimize_wasm_if_available(wasm: &Path, label: &str) -> Result<()> {
    if !pages_wasm_opt_enabled() {
        eprintln!("wasm-opt skipped for {label}: disabled by {PAGES_WASM_OPT_ENV}");
        return Ok(());
    }

    let Some(wasm_opt) = find_wasm_opt() else {
        eprintln!("wasm-opt skipped for {label}: install Binaryen or expose EMSDK/upstream/bin");
        return Ok(());
    };

    let before = file_size(wasm)?;
    let tmp = wasm.with_extension("wasm-opt.tmp");
    let mut command = Command::new(wasm_opt);
    command
        .arg("-Oz")
        .arg("--enable-bulk-memory")
        .arg("--enable-bulk-memory-opt")
        .arg("--enable-nontrapping-float-to-int")
        .arg("--strip-debug")
        .arg("--strip-producers")
        .arg(wasm)
        .arg("-o")
        .arg(&tmp);
    run_command(&mut command, &format!("optimize {label} with wasm-opt"))?;

    fs::copy(&tmp, wasm)?;
    fs::remove_file(&tmp)?;

    let after = file_size(wasm)?;
    let saved = before.saturating_sub(after);
    let pct = if before == 0 {
        0.0
    } else {
        saved as f64 * 100.0 / before as f64
    };
    eprintln!(
        "{label} optimized: {} -> {} ({saved} bytes saved, {pct:.1}%)",
        format_bytes(before),
        format_bytes(after)
    );
    Ok(())
}

fn pages_wasm_opt_enabled() -> bool {
    !matches!(
        env::var(PAGES_WASM_OPT_ENV).ok().as_deref(),
        Some("0" | "false" | "False" | "FALSE" | "off" | "OFF" | "no" | "NO")
    )
}

fn file_size(path: &Path) -> Result<u64> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(Into::into)
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= MIB {
        format!("{:.2} MiB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.2} KiB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn find_wasm_opt() -> Option<PathBuf> {
    if let Some(path) = runnable_tool("wasm-opt", "--version") {
        return Some(path);
    }

    let Ok(emsdk) = env::var("EMSDK") else {
        return None;
    };

    let bin_dir = PathBuf::from(emsdk).join("upstream").join("bin");
    for name in ["wasm-opt", "wasm-opt.exe"] {
        let candidate = bin_dir.join(name);
        if candidate.exists()
            && let Some(path) = runnable_path(&candidate, "--version")
        {
            return Some(path);
        }
    }

    None
}

fn ensure_tool(name: &str, arg: &str, message: &str) -> Result<()> {
    if runnable_tool(name, arg).is_some() {
        Ok(())
    } else {
        Err(format!("{message}: `{name} {arg}` failed").into())
    }
}

fn runnable_tool(name: &str, arg: &str) -> Option<PathBuf> {
    runnable_path(Path::new(name), arg).map(|_| PathBuf::from(name))
}

fn runnable_path(path: &Path, arg: &str) -> Option<PathBuf> {
    Command::new(path)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .filter(|status| status.success())
        .map(|_| path.to_path_buf())
}

fn run_command(command: &mut Command, label: &str) -> Result<()> {
    eprintln!("running {label}: {command:?}");
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed with status {status}").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn temp_root() -> TempRoot {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let path =
            env::temp_dir().join(format!("boxddd-xtask-test-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("failed to create temporary test root");
        TempRoot { path }
    }

    fn write_empty_file(root: &Path, path: &str) {
        write_text_file(root, path, "");
    }

    fn write_text_file(root: &Path, path: &str, contents: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("test file should have a parent"))
            .expect("failed to create temporary test parent");
        fs::write(path, contents).expect("failed to write temporary test file");
    }

    fn parity_row(mode: &str, target: &str) -> SampleParityRow {
        SampleParityRow {
            category: "Collision".to_string(),
            name: "Shape Cast".to_string(),
            source: "sample_collision.cpp:1".to_string(),
            mode: SampleParityMode::try_from(mode).expect("test parity mode should be valid"),
            target: target.to_string(),
            notes: "covered by tests".to_string(),
            line_number: 42,
        }
    }

    #[test]
    fn build_profile_parses_supported_values() {
        assert!(matches!(
            BuildProfile::parse("debug"),
            Some(BuildProfile::Debug)
        ));
        assert!(matches!(
            BuildProfile::parse("release"),
            Some(BuildProfile::Release)
        ));
        assert!(matches!(
            BuildProfile::parse("wasm-release"),
            Some(BuildProfile::WasmRelease)
        ));
        assert!(matches!(
            BuildProfile::parse("WASM_RELEASE"),
            Some(BuildProfile::WasmRelease)
        ));
        assert!(BuildProfile::parse("fast").is_none());
    }

    #[test]
    fn wasm_release_profile_uses_custom_cargo_profile() {
        assert_eq!(
            BuildProfile::WasmRelease.cargo_args(),
            &["--profile", "wasm-release"]
        );
        assert_eq!(BuildProfile::WasmRelease.target_dir(), "wasm-release");
    }

    #[test]
    fn format_bytes_uses_binary_units() {
        assert_eq!(format_bytes(31), "31 B");
        assert_eq!(format_bytes(1536), "1.50 KiB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.00 MiB");
    }

    #[test]
    fn bevy_loader_releases_consumer_only_on_real_unload() {
        let loader = bevy_testbed_loader_js();

        assert!(loader.contains(
            r#"window.addEventListener("pagehide", (event) => {
    if (!event.persisted) {
      releaseBoxdddConsumer(bevyExports);
    }
  });"#
        ));
        assert!(!loader.contains("{ once: true }"));
    }

    #[test]
    fn official_sample_inventory_requires_the_generated_contract_commit() {
        let source = r#"{
            "schema_version": 1,
            "upstream_commit": "0000000000000000000000000000000000000000",
            "samples": [{"category":"Bodies","name":"Body Type","source":"sample_bodies.cpp:1"}]
        }"#;

        let error = parse_official_sample_inventory(Path::new("inventory.json"), source)
            .expect_err("a stale inventory must be rejected")
            .to_string();

        assert!(error.contains(UPSTREAM_COMMIT));
    }

    #[test]
    fn official_sample_inventory_requires_sorted_unique_samples() {
        let source = format!(
            r#"{{
                "schema_version": 1,
                "upstream_commit": "{UPSTREAM_COMMIT}",
                "samples": [
                    {{"category":"World","name":"Far Stack","source":"sample_world.cpp:2"}},
                    {{"category":"Bodies","name":"Body Type","source":"sample_bodies.cpp:1"}}
                ]
            }}"#
        );

        let error = parse_official_sample_inventory(Path::new("inventory.json"), &source)
            .expect_err("an unsorted inventory must be rejected")
            .to_string();

        assert!(error.contains("is not sorted"));
    }

    #[test]
    fn provider_exports_always_include_the_abi_revision_sentinel() {
        let root = temp_root();
        let path = write_exports_json(
            root.path(),
            &[
                "b3CreateWorld".to_string(),
                "boxddd_provider_install_default_pre_solve".to_string(),
            ],
        )
        .expect("provider export inventory should be written");
        let exports = fs::read_to_string(path).expect("provider exports should be readable");

        assert!(exports.contains("_b3CreateWorld"));
        assert!(exports.contains("_boxddd_provider_install_default_pre_solve"));
        assert!(exports.contains("_boxddd_provider_abi_revision"));
    }

    #[test]
    fn test_only_parity_rows_require_a_tests_target() {
        let root = temp_root();
        write_empty_file(root.path(), "boxddd/examples/shape_queries.rs");

        let row = parity_row("TestOnly", "`boxddd/examples/shape_queries.rs`");
        let error = validate_sample_parity_row(root.path(), &row)
            .expect_err("TestOnly rows should require a tests/ target")
            .to_string();

        assert!(error.contains("TestOnly target must include"));
    }

    #[test]
    fn test_only_parity_rows_accept_mixed_example_and_tests_targets() {
        let root = temp_root();
        write_empty_file(root.path(), "boxddd/examples/shape_queries.rs");
        write_empty_file(root.path(), "boxddd/tests/world_and_queries.rs");

        let row = parity_row(
            "TestOnly",
            "`boxddd/examples/shape_queries.rs`, `boxddd/tests/world_and_queries.rs`",
        );

        validate_sample_parity_row(root.path(), &row)
            .expect("TestOnly row should accept a tests/ path");
    }

    #[test]
    fn visual_parity_rows_do_not_require_tests_targets() {
        let root = temp_root();
        write_empty_file(root.path(), "bevy_boxddd/examples/testbed_3d/scenes.rs");

        let row = parity_row(
            "TeachingAdaptation",
            "`bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders`",
        );

        validate_sample_parity_row(root.path(), &row)
            .expect("visual parity rows should only need existing targets");
    }
}
